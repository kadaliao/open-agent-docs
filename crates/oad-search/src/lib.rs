//! 全文搜索：tantivy + jieba 分词（中文按词，英文转小写、取词干）。
//!
//! 每条文档是一个页面的标题、正文里的一段（顶层块），或者数据库一行的属性。按工作区、种类过滤；
//! 片段由这里切、用 `<mark>` 标出命中的词（最后一个词按前缀算，边打边搜也有结果）。
//! 索引目录里另存一份“标记”（每页、每个库索引到哪个版本），启动时对比着补漏掉的，不用全量重建。
use jieba_rs::{Jieba, TokenizeMode};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, BoostQuery, FuzzyTermQuery, Occur, Query, TermQuery};
use tantivy::schema::{
    Field, IndexRecordOption, STORED, STRING, Schema, TextFieldIndexing, TextOptions, Value,
};
use tantivy::tokenizer::{
    Language, LowerCaser, RemoveLongFilter, Stemmer, TextAnalyzer, Token, TokenStream, Tokenizer,
};
use tantivy::{IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term, doc};

const TOKENIZER: &str = "oad";
/// 标题比正文重要几倍。
const TITLE_BOOST: f32 = 3.0;
/// 片段大约多长（字）。
const SNIPPET_CHARS: usize = 80;
/// 一次搜索最多看多少条命中。
const MAX_FETCH: usize = 8000;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("搜索索引出错：{0}")]
    Index(#[from] tantivy::TantivyError),
    #[error("搜索索引出错：{0}")]
    Io(#[from] std::io::Error),
    #[error("搜索索引出错：{0}")]
    Dir(#[from] tantivy::directory::error::OpenDirectoryError),
}

pub type Result<T> = std::result::Result<T, Error>;

/// 分词词典：jieba 自带的 dict.txt（MIT）去掉词频 ≤ 3 的生僻词（35 万 → 15 万词）和词性，zstd 压缩。
/// 加载完占 17MB（完整词典 37MB）；去掉的多是长的复合词，搜索模式下照样切成短词，查得到。
/// 重新生成：`awk '$2>3{print $1" "$2}' dict.txt | zstd -19 -o dict/dict.txt.zst`；换了词典要加 TOKENIZER_VERSION。
static DICT: &[u8] = include_bytes!("../dict/dict.txt.zst");

/// 分词规则的版本：和索引目录里记的不一样就清掉重建（同一个词前后切法不同，旧索引就对不上了）。
const TOKENIZER_VERSION: &str = "2";

static JIEBA: LazyLock<Jieba> = LazyLock::new(|| {
    let raw = zstd::decode_all(DICT).expect("词典解压失败");
    Jieba::with_dict(&mut &raw[..]).expect("词典格式不对")
});

/// 分词器：jieba 的搜索模式（长词再切出短词），去掉空白和标点。
#[derive(Clone, Default)]
struct JiebaTokenizer {
    token: Token,
}

struct JiebaStream<'a> {
    tokens: std::vec::IntoIter<(usize, usize, &'a str)>,
    token: &'a mut Token,
    pos: usize,
}

impl Tokenizer for JiebaTokenizer {
    type TokenStream<'a> = JiebaStream<'a>;
    fn token_stream<'a>(&'a mut self, text: &'a str) -> JiebaStream<'a> {
        let tokens: Vec<(usize, usize, &str)> = JIEBA
            .tokenize(text, TokenizeMode::Search, true)
            .into_iter()
            .filter(|t| t.word.chars().any(char::is_alphanumeric))
            .map(|t| (t.byte_start, t.byte_end, t.word))
            .collect();
        self.token.reset();
        JiebaStream {
            tokens: tokens.into_iter(),
            token: &mut self.token,
            pos: 0,
        }
    }
}

impl TokenStream for JiebaStream<'_> {
    fn advance(&mut self) -> bool {
        match self.tokens.next() {
            Some((a, b, w)) => {
                self.token.offset_from = a;
                self.token.offset_to = b;
                self.token.position = self.pos;
                self.token.text.clear();
                self.token.text.push_str(w);
                self.pos += 1;
                true
            }
            None => false,
        }
    }
    fn token(&self) -> &Token {
        self.token
    }
    fn token_mut(&mut self) -> &mut Token {
        self.token
    }
}

fn analyzer() -> TextAnalyzer {
    TextAnalyzer::builder(JiebaTokenizer::default())
        .filter(RemoveLongFilter::limit(40))
        .filter(LowerCaser)
        .filter(Stemmer::new(Language::English))
        .build()
}

/// 分好的词：(在原文里的字节范围, 词)。
fn analyze(text: &str) -> Vec<(usize, usize, String)> {
    let mut a = analyzer();
    let mut s = a.token_stream(text);
    let mut out = Vec::new();
    while s.advance() {
        let t = s.token();
        out.push((t.offset_from, t.offset_to, t.text.clone()));
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Page,
    Row,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Page => "page",
            Kind::Row => "row",
        }
    }
    fn parse(s: &str) -> Kind {
        if s == "row" { Kind::Row } else { Kind::Page }
    }
}

/// 一条要索引的文档。
#[derive(Clone, Debug, PartialEq)]
pub struct Doc {
    pub page_id: String,
    /// 正文里的哪一块；None 是标题（或者数据库一行的属性）
    pub block_id: Option<String>,
    /// 行所在的库（删库时一起删）
    pub db_id: Option<String>,
    pub workspace_id: String,
    pub kind: Kind,
    /// 标题文档放标题，别的为空
    pub title: String,
    pub text: String,
}

impl Doc {
    fn key(&self) -> String {
        match &self.block_id {
            Some(b) => format!("{}:{b}", self.page_id),
            None => self.page_id.clone(),
        }
    }
}

/// 对索引的一个改动。
#[derive(Clone, Debug)]
pub enum Op {
    /// 加上或换掉（按页面 + 块认）
    Put(Doc),
    /// 删掉页面的一块
    DeleteBlock(String, String),
    /// 删掉一个页面的所有文档（标题、各段）
    DeletePage(String),
    /// 删掉一个库里所有行的文档
    DeleteDb(String),
}

/// 每页、每个库索引到了哪个版本（页面是 updated_at，库是 seq）。和索引存在一起。
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Marks {
    pub pages: HashMap<String, i64>,
    pub dbs: HashMap<String, i64>,
}

/// 搜索条件。
#[derive(Clone, Debug)]
pub struct SearchQuery<'a> {
    pub workspace_id: &'a str,
    pub text: &'a str,
    pub kind: Option<Kind>,
    pub limit: usize,
    /// 每个页面只给一条（分最高的那条的片段，`matches` 是这个页面命中了几条）；false 时每块一条（同一页最多两条）。
    pub per_page: bool,
}

/// 一条结果。
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Hit {
    pub page_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_id: Option<String>,
    pub kind: Kind,
    /// 带 `<mark>` 的片段（已经转义过 HTML）
    pub snippet: String,
    pub score: f32,
    /// 这个页面命中了几条（标题、正文的段）
    pub matches: usize,
}

struct Fields {
    key: Field,
    page: Field,
    block: Field,
    db: Field,
    ws: Field,
    kind: Field,
    title: Field,
    text: Field,
}

pub struct Index {
    index: tantivy::Index,
    reader: IndexReader,
    writer: Mutex<IndexWriter>,
    f: Fields,
    dir: Option<PathBuf>,
    marks: Mutex<Marks>,
}

fn schema() -> (Schema, Fields) {
    let mut b = Schema::builder();
    let text_opts = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer(TOKENIZER)
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();
    let f = Fields {
        key: b.add_text_field("key", STRING),
        page: b.add_text_field("page", STRING | STORED),
        block: b.add_text_field("block", STRING | STORED),
        db: b.add_text_field("db", STRING),
        ws: b.add_text_field("ws", STRING),
        kind: b.add_text_field("kind", STRING | STORED),
        title: b.add_text_field("title", text_opts.clone()),
        text: b.add_text_field("text", text_opts),
    };
    (b.build(), f)
}

impl Index {
    /// 打开（没有就建）`dir` 里的索引；不给目录就放内存里（测试、不想落盘时）。
    pub fn open(dir: Option<&Path>) -> Result<Index> {
        let (schema, f) = schema();
        let index = match dir {
            Some(d) => {
                // 分词规则变了：旧索引和标记一起清掉，启动时整体补一遍
                let ver = d.join("tokenizer");
                if d.join("meta.json").exists()
                    && std::fs::read_to_string(&ver).ok().as_deref() != Some(TOKENIZER_VERSION)
                {
                    std::fs::remove_dir_all(d)?;
                }
                std::fs::create_dir_all(d)?;
                std::fs::write(&ver, TOKENIZER_VERSION)?;
                let md = tantivy::directory::MmapDirectory::open(d)?;
                match tantivy::Index::open_or_create(md, schema.clone()) {
                    Ok(i) => i,
                    // 格式变了（升级了字段）：清掉重建，标记一起清掉
                    Err(_) => {
                        std::fs::remove_dir_all(d)?;
                        std::fs::create_dir_all(d)?;
                        std::fs::write(&ver, TOKENIZER_VERSION)?;
                        tantivy::Index::create_in_dir(d, schema)?
                    }
                }
            }
            None => tantivy::Index::create_in_ram(schema),
        };
        index.tokenizers().register(TOKENIZER, analyzer());
        // 一个写线程、20MB 缓冲：平时一次只改几段；灌 10 万段也不比 3 个线程（50MB）慢多少，峰值内存低得多
        let writer = index.writer_with_num_threads(1, 20_000_000)?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        let marks = dir
            .and_then(|d| std::fs::read(d.join("marks.json")).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Ok(Index {
            index,
            reader,
            writer: Mutex::new(writer),
            f,
            dir: dir.map(Path::to_path_buf),
            marks: Mutex::new(marks),
        })
    }

    /// 改索引、更新标记，提交后马上能搜到。标记在索引提交之后才存（中途崩了下次再补一遍，结果一样）。
    pub fn apply(&self, ops: Vec<Op>, marks: impl FnOnce(&mut Marks)) -> Result<()> {
        let mut w = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        for op in ops {
            match op {
                Op::Put(d) => {
                    w.delete_term(Term::from_field_text(self.f.key, &d.key()));
                    let mut td = doc!(
                        self.f.key => d.key(),
                        self.f.page => d.page_id.clone(),
                        self.f.ws => d.workspace_id.clone(),
                        self.f.kind => d.kind.as_str(),
                        self.f.title => d.title.clone(),
                        self.f.text => d.text.clone(),
                    );
                    if let Some(b) = &d.block_id {
                        td.add_text(self.f.block, b);
                    }
                    if let Some(db) = &d.db_id {
                        td.add_text(self.f.db, db);
                    }
                    w.add_document(td)?;
                }
                Op::DeleteBlock(p, b) => {
                    w.delete_term(Term::from_field_text(self.f.key, &format!("{p}:{b}")));
                }
                Op::DeletePage(p) => {
                    w.delete_term(Term::from_field_text(self.f.page, &p));
                }
                Op::DeleteDb(db) => {
                    w.delete_term(Term::from_field_text(self.f.db, &db));
                }
            }
        }
        w.commit()?;
        drop(w);
        self.reader.reload()?;
        let mut m = self.marks.lock().unwrap_or_else(|e| e.into_inner());
        marks(&mut m);
        if let Some(d) = &self.dir {
            let tmp = d.join("marks.json.tmp");
            std::fs::write(&tmp, serde_json::to_vec(&*m).unwrap_or_default())?;
            std::fs::rename(&tmp, d.join("marks.json"))?;
        }
        Ok(())
    }

    pub fn marks(&self) -> Marks {
        self.marks.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn num_docs(&self) -> u64 {
        self.reader.searcher().num_docs()
    }

    /// 搜。按页面合并（per_page）或者每块一条。
    pub fn search(&self, q: &SearchQuery) -> Result<Vec<Hit>> {
        let words = analyze(q.text);
        if words.is_empty() || q.limit == 0 {
            return Ok(vec![]);
        }
        // 每个词：标题或正文里有（最后一个词按前缀）；词之间都要有
        let prefix = !q.text.ends_with(char::is_whitespace);
        let n = words.len();
        let mut must: Vec<(Occur, Box<dyn Query>)> = Vec::new();
        let mut seen = HashSet::new();
        for (i, (_, _, w)) in words.iter().enumerate() {
            let last = i + 1 == n && prefix;
            if !seen.insert((w.clone(), last)) {
                continue;
            }
            let mut either: Vec<(Occur, Box<dyn Query>)> = Vec::new();
            for (field, boost) in [(self.f.title, TITLE_BOOST), (self.f.text, 1.0)] {
                let term = Term::from_field_text(field, w);
                let exact: Box<dyn Query> =
                    Box::new(TermQuery::new(term.clone(), IndexRecordOption::WithFreqs));
                either.push((Occur::Should, Box::new(BoostQuery::new(exact, boost))));
                if last {
                    let p: Box<dyn Query> = Box::new(FuzzyTermQuery::new_prefix(term, 0, false));
                    either.push((Occur::Should, Box::new(BoostQuery::new(p, boost * 0.5))));
                }
            }
            must.push((Occur::Must, Box::new(BooleanQuery::new(either))));
        }
        must.push((
            Occur::Must,
            Box::new(TermQuery::new(
                Term::from_field_text(self.f.ws, q.workspace_id),
                IndexRecordOption::Basic,
            )),
        ));
        if let Some(k) = q.kind {
            must.push((
                Occur::Must,
                Box::new(TermQuery::new(
                    Term::from_field_text(self.f.kind, k.as_str()),
                    IndexRecordOption::Basic,
                )),
            ));
        }
        let query = BooleanQuery::new(must);
        let searcher = self.reader.searcher();
        let terms: Vec<String> = words.iter().map(|w| w.2.clone()).collect();
        let max_per_page = if q.per_page { 1 } else { 2 };
        // 先取 limit 的 25 倍；同分的段落扎堆在几个页面里、凑不够 limit 个页面时，再多取一些
        let mut fetch = (q.limit * 25).min(1000);
        loop {
            let top = searcher.search(&query, &TopDocs::with_limit(fetch).order_by_score())?;
            let full = top.len() == fetch;
            let out = self.collect(&searcher, top, q, &terms, prefix, max_per_page)?;
            if out.len() >= q.limit || !full || fetch >= MAX_FETCH {
                return Ok(out);
            }
            fetch = (fetch * 4).min(MAX_FETCH);
        }
    }

    /// 把命中的文档按页面合并、切片段。
    fn collect(
        &self,
        searcher: &tantivy::Searcher,
        top: Vec<(f32, tantivy::DocAddress)>,
        q: &SearchQuery,
        terms: &[String],
        prefix: bool,
        max_per_page: usize,
    ) -> Result<Vec<Hit>> {
        let mut per_page: HashMap<String, (usize, Option<usize>)> = HashMap::new();
        let mut out: Vec<Hit> = Vec::new();
        for (score, addr) in top {
            let d: TantivyDocument = searcher.doc(addr)?;
            let get = |f: Field| d.get_first(f).and_then(|v| v.as_str()).map(str::to_string);
            let page = get(self.f.page).unwrap_or_default();
            let c = per_page.entry(page.clone()).or_default();
            c.0 += 1;
            if let Some(i) = c.1 {
                out[i].matches = c.0;
            }
            if c.0 > max_per_page || out.len() >= q.limit {
                continue;
            }
            if q.per_page {
                c.1 = Some(out.len());
            }
            let title = get(self.f.title).unwrap_or_default();
            let text = get(self.f.text).unwrap_or_default();
            let snippet = if !title.is_empty() && highlight(&title, terms, prefix).1 {
                highlight(&title, terms, prefix).0
            } else {
                highlight(&text, terms, prefix).0
            };
            out.push(Hit {
                page_id: page,
                block_id: get(self.f.block),
                kind: Kind::parse(&get(self.f.kind).unwrap_or_default()),
                snippet,
                score,
                matches: 1,
            });
        }
        Ok(out)
    }

    /// 清空（测试用）。
    pub fn clear(&self) -> Result<()> {
        let mut w = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        w.delete_all_documents()?;
        w.commit()?;
        drop(w);
        self.reader.reload()?;
        *self.marks.lock().unwrap_or_else(|e| e.into_inner()) = Marks::default();
        Ok(())
    }

    #[doc(hidden)]
    pub fn schema_fields(&self) -> usize {
        self.index.schema().fields().count()
    }
}

fn escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '&' => o.push_str("&amp;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            c => o.push(c),
        }
    }
    o
}

/// 切片段、标出命中的词：返回 (HTML, 有没有命中)。片段取第一个命中附近，太长两头用 … 省略。
pub fn highlight(text: &str, terms: &[String], prefix_last: bool) -> (String, bool) {
    let toks = analyze(text);
    let last = terms.last();
    let hit = |w: &str| {
        terms.iter().any(|t| t == w)
            || (prefix_last && last.is_some_and(|l| w.starts_with(l.as_str())))
    };
    // 搜索模式会切出重叠的词（“共和国”和“共和”），挨着的命中也连起来
    let mut marks: Vec<(usize, usize)> = Vec::new();
    for (a, b, w) in &toks {
        if hit(w) {
            if let Some(m) = marks.last_mut()
                && *a <= m.1
            {
                m.1 = m.1.max(*b);
                continue;
            }
            marks.push((*a, *b));
        }
    }
    let found = !marks.is_empty();
    // 窗口：第一个命中前面留一点
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let total = chars.len();
    let first = marks.first().map(|m| m.0).unwrap_or(0);
    let first_char = chars.iter().position(|(i, _)| *i >= first).unwrap_or(0);
    let start_char = first_char.saturating_sub(SNIPPET_CHARS / 4);
    let end_char = (start_char + SNIPPET_CHARS).min(total);
    let start_char = end_char.saturating_sub(SNIPPET_CHARS).min(start_char);
    let byte = |ci: usize| chars.get(ci).map(|c| c.0).unwrap_or(text.len());
    let (s, e) = (byte(start_char), byte(end_char));
    let mut out = String::new();
    if start_char > 0 {
        out.push('…');
    }
    let mut at = s;
    for (a, b) in marks.iter().filter(|(a, b)| *a >= s && *b <= e) {
        out.push_str(&escape(&text[at..*a]));
        out.push_str("<mark>");
        out.push_str(&escape(&text[*a..*b]));
        out.push_str("</mark>");
        at = *b;
    }
    out.push_str(&escape(&text[at..e]));
    if end_char < total {
        out.push('…');
    }
    (out.split_whitespace().collect::<Vec<_>>().join(" "), found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(page: &str, block: Option<&str>, ws: &str, title: &str, text: &str) -> Doc {
        Doc {
            page_id: page.into(),
            block_id: block.map(str::to_string),
            db_id: None,
            workspace_id: ws.into(),
            kind: Kind::Page,
            title: title.into(),
            text: text.into(),
        }
    }

    fn q<'a>(ws: &'a str, text: &'a str) -> SearchQuery<'a> {
        SearchQuery {
            workspace_id: ws,
            text,
            kind: None,
            limit: 10,
            per_page: false,
        }
    }

    #[test]
    fn chinese_words_english_stems_title_boost() {
        let ix = Index::open(None).unwrap();
        ix.apply(
            vec![
                Op::Put(doc("p1", None, "w", "季度计划", "")),
                Op::Put(doc(
                    "p1",
                    Some("b1"),
                    "w",
                    "",
                    "我们下个季度要完成搜索功能的设计。",
                )),
                Op::Put(doc(
                    "p2",
                    Some("b1"),
                    "w",
                    "",
                    "The search engine is running fast. 计划",
                )),
                Op::Put(doc("p3", Some("b1"), "other", "", "季度计划在别的工作区")),
            ],
            |_| {},
        )
        .unwrap();
        let hits = ix.search(&q("w", "计划")).unwrap();
        assert_eq!(hits[0].page_id, "p1", "标题命中排前面：{hits:?}");
        assert!(hits[0].snippet.contains("<mark>计划</mark>"), "{hits:?}");
        assert!(hits.iter().all(|h| h.page_id != "p3"), "别的工作区不出来");
        // 英文取词干：runs 能搜到 running
        let hits = ix.search(&q("w", "runs ")).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].snippet.contains("<mark>running</mark>"), "{hits:?}");
        // 边打边搜：最后一个词按前缀
        let hits = ix.search(&q("w", "sear")).unwrap();
        assert!(hits.iter().any(|h| h.page_id == "p2"), "{hits:?}");
        assert!(
            ix.search(&q("w", "季度 设计"))
                .unwrap()
                .iter()
                .all(|h| h.page_id == "p1")
        );
        // 按页面合并：标题和正文都命中的 p1 只给一条，matches 是 2
        let hits = ix
            .search(&SearchQuery {
                per_page: true,
                ..q("w", "季度")
            })
            .unwrap();
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert_eq!(hits[0].matches, 2);
        // 删页
        ix.apply(vec![Op::DeletePage("p1".into())], |_| {}).unwrap();
        assert!(ix.search(&q("w", "季度")).unwrap().is_empty());
    }

    #[test]
    fn marks_and_reopen() {
        let dir = tempfile::tempdir().unwrap();
        {
            let ix = Index::open(Some(dir.path())).unwrap();
            ix.apply(
                vec![Op::Put(doc("p", Some("b"), "w", "", "持久化的索引"))],
                |m| {
                    m.pages.insert("p".into(), 7);
                },
            )
            .unwrap();
        }
        let ix = Index::open(Some(dir.path())).unwrap();
        assert_eq!(ix.marks().pages["p"], 7);
        assert_eq!(ix.search(&q("w", "索引")).unwrap().len(), 1);
        // 换掉同一块
        ix.apply(
            vec![Op::Put(doc("p", Some("b"), "w", "", "换了内容"))],
            |_| {},
        )
        .unwrap();
        assert!(ix.search(&q("w", "索引")).unwrap().is_empty());
        assert_eq!(ix.num_docs(), 1);
    }

    #[test]
    fn snippet_window_and_escape() {
        let long = format!(
            "{}<b>关键词</b>{}",
            "前面的字".repeat(30),
            "后面的字".repeat(30)
        );
        let (s, found) = highlight(&long, &["关键词".into()], false);
        assert!(found);
        assert!(s.starts_with('…') && s.ends_with('…'), "{s}");
        assert!(s.contains("&lt;b&gt;<mark>关键词</mark>&lt;/b&gt;"), "{s}");
        assert!(s.chars().count() < 120);
    }
}
