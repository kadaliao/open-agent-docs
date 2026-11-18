//! 网页解析：字符集、Open Graph / Twitter Card / <title> / JSON-LD / 图标 / oEmbed 发现。

use std::sync::LazyLock;

use encoding_rs::{Encoding, UTF_8};
use regex::bytes::Regex as BytesRegex;
use scraper::{Html, Selector};
use url::Url;

use crate::model::{Author, Image, Kind, Media, MediaKind, Provider, Source, Unfurl};

/// 按 HTTP 头 → BOM → <meta charset> → UTF-8 的顺序定字符集，再解码。国内不少站点还是 GBK / GB2312。
pub fn decode(body: &[u8], content_type: Option<&str>) -> (String, &'static Encoding) {
    if let Some((enc, bom_len)) = Encoding::for_bom(body) {
        return (enc.decode_without_bom_handling(&body[bom_len..]).0.into_owned(), enc);
    }
    let from_header = content_type.and_then(charset_param).and_then(|l| Encoding::for_label(l.as_bytes()));
    let enc = from_header.or_else(|| sniff_meta_charset(body)).unwrap_or(UTF_8);
    // 网页里写 UTF-16 的 meta 按规范当 UTF-8 处理；GB2312 / GBK 由 encoding_rs 统一成 GBK（GB18030 的子集）。
    let enc = if enc == encoding_rs::UTF_16LE || enc == encoding_rs::UTF_16BE { UTF_8 } else { enc };
    (enc.decode_without_bom_handling(body).0.into_owned(), enc)
}

fn charset_param(ct: &str) -> Option<String> {
    ct.split(';').skip(1).find_map(|p| {
        let (k, v) = p.split_once('=')?;
        k.trim().eq_ignore_ascii_case("charset").then(|| v.trim().trim_matches(['"', '\'']).to_string())
    })
}

fn sniff_meta_charset(body: &[u8]) -> Option<&'static Encoding> {
    static RE: LazyLock<BytesRegex> = LazyLock::new(|| BytesRegex::new(r#"(?i)<meta[^>]+charset\s*=\s*["']?\s*([a-z0-9_\-:.]+)"#).unwrap());
    let head = &body[..body.len().min(4096)];
    let cap = RE.captures(head)?;
    Encoding::for_label(cap.get(1)?.as_bytes())
}

#[derive(Debug, Clone)]
pub struct Icon {
    pub href: String,
    pub rel: String,
    pub size: Option<u32>,
    pub mime: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PageMeta {
    /// (property 或 name 小写, content)，按出现顺序。
    pub metas: Vec<(String, String)>,
    pub title_tag: Option<String>,
    pub lang: Option<String>,
    pub canonical: Option<String>,
    pub icons: Vec<Icon>,
    pub oembed_json: Option<String>,
    pub jsonld: Vec<serde_json::Value>,
    base: Option<Url>,
}

macro_rules! sel {
    ($s:expr) => {{
        static S: LazyLock<Selector> = LazyLock::new(|| Selector::parse($s).unwrap());
        &*S
    }};
}

impl PageMeta {
    pub fn parse(html: &str, page_url: &Url) -> Self {
        let doc = Html::parse_document(html);
        let mut pm = PageMeta::default();
        let base = doc
            .select(sel!("base[href]"))
            .next()
            .and_then(|b| b.value().attr("href"))
            .and_then(|h| page_url.join(h).ok())
            .unwrap_or_else(|| page_url.clone());
        pm.base = Some(base);
        for m in doc.select(sel!("meta[content]")) {
            let v = m.value();
            let key = v.attr("property").or_else(|| v.attr("name")).or_else(|| v.attr("itemprop"));
            if let (Some(k), Some(c)) = (key, v.attr("content")) {
                let c = clean(c);
                if !c.is_empty() {
                    pm.metas.push((k.trim().to_ascii_lowercase(), c));
                }
            }
        }
        pm.title_tag = doc.select(sel!("title")).next().map(|t| clean(&t.text().collect::<String>())).filter(|s| !s.is_empty());
        pm.lang = doc
            .select(sel!("html[lang]"))
            .next()
            .and_then(|h| h.value().attr("lang"))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        for l in doc.select(sel!("link[rel][href]")) {
            let v = l.value();
            let rel = v.attr("rel").unwrap_or_default().to_ascii_lowercase();
            let href = v.attr("href").unwrap_or_default();
            let ty = v.attr("type").map(|t| t.to_ascii_lowercase());
            let rels: Vec<&str> = rel.split_whitespace().collect();
            if rels.contains(&"canonical") && pm.canonical.is_none() {
                pm.canonical = pm.abs(href);
            }
            if rels.contains(&"alternate") && ty.as_deref() == Some("application/json+oembed") && pm.oembed_json.is_none() {
                pm.oembed_json = pm.abs(href);
            }
            if rels.iter().any(|r| *r == "icon" || *r == "apple-touch-icon" || *r == "apple-touch-icon-precomposed")
                && let Some(h) = pm.abs(href)
            {
                let size = v
                    .attr("sizes")
                    .and_then(|s| s.split_whitespace().filter_map(|x| x.split(['x', 'X']).next()?.parse::<u32>().ok()).max());
                pm.icons.push(Icon { href: h, rel: rel.clone(), size, mime: ty });
            }
        }
        for s in doc.select(sel!(r#"script[type="application/ld+json"]"#)) {
            let txt: String = s.text().collect();
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(txt.trim()) {
                pm.jsonld.push(v);
            }
        }
        pm
    }

    /// 相对地址转绝对地址，只认 http(s)。
    pub fn abs(&self, href: &str) -> Option<String> {
        let href = href.trim();
        if href.is_empty() || href.starts_with("data:") || href.starts_with("javascript:") {
            return None;
        }
        let u = self.base.as_ref()?.join(href).ok()?;
        matches!(u.scheme(), "http" | "https").then(|| u.to_string())
    }

    pub fn meta(&self, keys: &[&str]) -> Option<&str> {
        keys.iter().find_map(|k| self.metas.iter().find(|(mk, _)| mk == k).map(|(_, v)| v.as_str()))
    }

    fn meta_u32(&self, keys: &[&str]) -> Option<u32> {
        self.meta(keys).and_then(|v| v.trim().parse().ok())
    }

    pub fn favicon(&self) -> Option<String> {
        let score = |i: &Icon| -> i32 {
            let mut s = match i.size {
                Some(sz) if sz > 256 => 100,
                Some(sz) => sz as i32,
                None if i.mime.as_deref() == Some("image/svg+xml") || i.href.ends_with(".svg") => 200,
                None if i.rel.contains("apple-touch-icon") => 180,
                None => 32,
            };
            if i.rel.contains("mask-icon") {
                s -= 1000;
            }
            s
        };
        if let Some(best) = self.icons.iter().max_by_key(|i| score(i)) {
            return Some(best.href.clone());
        }
        self.abs("/favicon.ico")
    }

    /// JSON-LD 里第一个像“文章 / 视频 / 页面”的对象。
    fn jsonld_main(&self) -> Option<&serde_json::Value> {
        fn walk(v: &serde_json::Value) -> Option<&serde_json::Value> {
            match v {
                serde_json::Value::Array(a) => a.iter().find_map(walk),
                serde_json::Value::Object(o) => {
                    if let Some(g) = o.get("@graph") {
                        return walk(g);
                    }
                    let ty = o.get("@type").map(|t| t.to_string()).unwrap_or_default();
                    let interesting = ["Article", "NewsArticle", "BlogPosting", "VideoObject", "WebPage", "Report", "TechArticle"];
                    interesting.iter().any(|i| ty.contains(i)).then_some(v)
                }
                _ => None,
            }
        }
        self.jsonld.iter().find_map(walk)
    }

    /// 用网页 meta 拼一张通用卡片。
    pub fn to_unfurl(&self, input_url: &str, final_url: &Url) -> Unfurl {
        let og_type = self.meta(&["og:type"]).unwrap_or_default().to_ascii_lowercase();
        let kind = if og_type.starts_with("video") {
            Kind::Video
        } else if og_type == "article"
            || self
                .jsonld_main()
                .is_some_and(|v| v.get("@type").is_some_and(|t| t.to_string().contains("Article") || t.to_string().contains("BlogPosting")))
        {
            Kind::Article
        } else {
            Kind::Bookmark
        };
        let mut u = Unfurl::new(kind, Provider::Generic, Source::Html, input_url);
        let ld = self.jsonld_main();
        let ld_str = |k: &str| ld.and_then(|v| v.get(k)).and_then(|x| x.as_str()).map(clean);

        u.canonical_url =
            self.meta(&["og:url"]).and_then(|h| self.abs(h)).or_else(|| self.canonical.clone()).or_else(|| Some(final_url.to_string()));
        u.title =
            self.meta(&["og:title", "twitter:title"]).map(str::to_string).or_else(|| ld_str("headline")).or_else(|| self.title_tag.clone());
        u.description = self
            .meta(&["og:description", "twitter:description", "description"])
            .map(|d| truncate(d, 500))
            .or_else(|| ld_str("description").map(|d| truncate(&d, 500)));
        u.site_name = self
            .meta(&["og:site_name", "application-name", "apple-mobile-web-app-title"])
            .map(str::to_string)
            .or_else(|| final_url.host_str().map(|h| h.trim_start_matches("www.").to_string()));
        u.favicon = self.favicon();
        u.lang = self.lang.clone().or_else(|| self.meta(&["og:locale"]).map(|l| l.replace('_', "-")));
        u.published_at = self
            .meta(&["article:published_time", "og:article:published_time", "datepublished", "pubdate", "publishdate", "date"])
            .map(str::to_string)
            .or_else(|| ld_str("datePublished").or_else(|| ld_str("uploadDate")));

        let mut author = Author {
            name: self.meta(&["author", "article:author"]).filter(|a| !a.starts_with("http")).map(str::to_string),
            ..Default::default()
        };
        if author.name.is_none()
            && let Some(a) = ld.and_then(|v| v.get("author"))
        {
            let first = if let Some(arr) = a.as_array() { arr.first() } else { Some(a) };
            author.name = first.and_then(|x| x.get("name").and_then(|n| n.as_str()).or(x.as_str())).map(clean);
            author.url = first.and_then(|x| x.get("url")).and_then(|n| n.as_str()).map(str::to_string);
        }
        if let Some(h) = self.meta(&["twitter:creator"]) {
            author.handle = Some(h.trim_start_matches('@').to_string());
        }
        if !author.is_empty() {
            u.author = Some(author);
        }

        if let Some(img) = self
            .meta(&["og:image:secure_url", "og:image", "og:image:url", "twitter:image", "twitter:image:src", "image", "thumbnailurl"])
            .and_then(|h| self.abs(h))
        {
            u.thumbnail = Some(Image {
                url: img,
                width: self.meta_u32(&["og:image:width", "twitter:image:width"]),
                height: self.meta_u32(&["og:image:height", "twitter:image:height"]),
                alt: self.meta(&["og:image:alt", "twitter:image:alt"]).map(str::to_string),
            });
        }
        if let Some(v) = self.meta(&["og:video:secure_url", "og:video:url", "og:video"]).and_then(|h| self.abs(h)) {
            u.media.push(Media {
                kind: MediaKind::Video,
                url: v,
                mime: self.meta(&["og:video:type"]).map(str::to_string),
                width: self.meta_u32(&["og:video:width"]),
                height: self.meta_u32(&["og:video:height"]),
                poster: u.thumbnail.as_ref().map(|t| t.url.clone()),
                duration_ms: None,
                alt: None,
            });
        }
        u
    }
}

/// 合并空白、去首尾空格。
pub fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn truncate(s: &str, max_chars: usize) -> String {
    let s = clean(s);
    if s.chars().count() <= max_chars {
        return s;
    }
    let mut out: String = s.chars().take(max_chars).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gbk_meta_charset() {
        let (gbk_bytes, _, _) = encoding_rs::GBK.encode("<html><head><meta charset=\"gb2312\"><title>中文标题</title></head></html>");
        let (s, enc) = decode(&gbk_bytes, Some("text/html"));
        assert_eq!(enc, encoding_rs::GBK);
        assert!(s.contains("中文标题"));
        // http-equiv 写法
        let (b2, _, _) = encoding_rs::GBK.encode(r#"<meta http-equiv="Content-Type" content="text/html; charset=GBK"><title>新闻</title>"#);
        assert!(decode(&b2, None).0.contains("新闻"));
        // 头里的 charset 优先
        let (b3, _, _) = encoding_rs::GBK.encode("<title>你好</title>");
        assert!(decode(&b3, Some("text/html; charset=\"gbk\"")).0.contains("你好"));
    }

    #[test]
    fn og_and_fallbacks() {
        let html = r#"<!doctype html><html lang="zh-CN"><head>
            <base href="https://cdn.example.com/a/">
            <title>  Tag   Title </title>
            <meta property="og:title" content="OG &amp; Title">
            <meta name="description" content="desc">
            <meta property="og:image" content="img/cover.png">
            <meta property="og:image:width" content="1200">
            <meta name="twitter:creator" content="@alice">
            <meta property="article:published_time" content="2025-01-02T03:04:05Z">
            <link rel="icon" href="/fav-32.png" sizes="32x32">
            <link rel="apple-touch-icon" href="/apple.png" sizes="180x180">
            <link rel="canonical" href="https://example.com/post">
            <link rel="alternate" type="application/json+oembed" href="/oembed?url=x">
            <script type="application/ld+json">{"@context":"https://schema.org","@type":"NewsArticle","author":[{"name":"Bob"}]}</script>
            </head><body></body></html>"#;
        let page = Url::parse("https://example.com/post?utm=1").unwrap();
        let pm = PageMeta::parse(html, &page);
        let u = pm.to_unfurl(page.as_str(), &page);
        assert_eq!(u.title.as_deref(), Some("OG & Title"));
        assert_eq!(u.description.as_deref(), Some("desc"));
        assert_eq!(u.kind, Kind::Article);
        assert_eq!(u.thumbnail.as_ref().unwrap().url, "https://cdn.example.com/a/img/cover.png");
        assert_eq!(u.thumbnail.as_ref().unwrap().width, Some(1200));
        assert_eq!(u.favicon.as_deref(), Some("https://cdn.example.com/apple.png"));
        assert_eq!(u.canonical_url.as_deref(), Some("https://example.com/post"));
        assert_eq!(u.author.as_ref().unwrap().name.as_deref(), Some("Bob"));
        assert_eq!(u.author.as_ref().unwrap().handle.as_deref(), Some("alice"));
        assert_eq!(u.lang.as_deref(), Some("zh-CN"));
        assert_eq!(pm.oembed_json.as_deref(), Some("https://cdn.example.com/oembed?url=x"));
    }

    #[test]
    fn title_only_page_and_default_favicon() {
        let page = Url::parse("https://example.org/x").unwrap();
        let pm = PageMeta::parse("<title>Hello</title>", &page);
        let u = pm.to_unfurl(page.as_str(), &page);
        assert_eq!(u.title.as_deref(), Some("Hello"));
        assert_eq!(u.favicon.as_deref(), Some("https://example.org/favicon.ico"));
        assert_eq!(u.site_name.as_deref(), Some("example.org"));
        assert_eq!(u.kind, Kind::Bookmark);
    }
}
