//! 把结果写成给模型读的文字。
use crate::OpKind;
use crate::i18n::{agent_name, kv_line, list_sep, quote, tr};
use oad_core::i18n::Lang;
use oad_core::{Error, Node, Page, PrincipalKind, ThreadView};
use serde_json::Value;
use std::collections::HashMap;

pub fn excerpt(s: &str, n: usize) -> String {
    let s = s.replace('\n', " ");
    let t: String = s.chars().take(n).collect();
    if t.chars().count() < s.chars().count() {
        format!("{t}…")
    } else {
        t
    }
}

/// 毫秒时间戳 → "2026-11-18 13:05 UTC"。
pub fn fmt_time(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // 公历换算（Howard Hinnant 的 civil_from_days）
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02} UTC",
        rem / 3600,
        rem % 3600 / 60
    )
}

fn title(lang: Lang, p: &Page) -> String {
    if p.title.is_empty() {
        tr!(lang, "（无标题）")
    } else {
        p.title.clone()
    }
}

fn who(lang: Lang, kind: PrincipalKind, name: &str) -> String {
    let n = if name.is_empty() {
        tr!(lang, "（不知道是谁）")
    } else {
        name.to_string()
    };
    match kind {
        PrincipalKind::Agent => agent_name(lang, &n),
        PrincipalKind::User => n,
    }
}

pub fn render_search(lang: Lang, q: &str, hits: &[oad_core::search::SearchResult]) -> String {
    if hits.is_empty() {
        return tr!(lang, "没有找到和「{q}」有关的页面。", q = q);
    }
    let mut s = tr!(lang, "找到 {n} 条：\n", n = hits.len());
    for h in hits {
        let kind = if h.kind == "row" {
            tr!(lang, "（数据库的行）")
        } else {
            String::new()
        };
        let t = if h.title.trim().is_empty() {
            tr!(lang, "无标题")
        } else {
            h.title.clone()
        };
        s.push_str(&tr!(
            lang,
            "- 《{title}》{kind} page_id: {id}",
            title = t,
            kind = kind,
            id = h.page_id
        ));
        if let Some(b) = &h.block_id {
            s.push_str(&tr!(lang, " · 块 ⟦{b}⟧", b = b));
        }
        s.push('\n');
        let snip = unmark(&h.snippet);
        if !snip.trim().is_empty() {
            s.push_str(&format!("  {snip}\n"));
        }
    }
    s
}

/// 片段的 HTML（`<mark>` 和转义）→ 给模型看的文字（命中的词加粗）。
fn unmark(s: &str) -> String {
    s.replace("<mark>", "**")
        .replace("</mark>", "**")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

pub fn render_tree(lang: Lang, pages: &[Page], root: Option<&str>) -> String {
    let mut kids: HashMap<Option<&str>, Vec<&Page>> = HashMap::new();
    for p in pages {
        kids.entry(p.parent_id.as_deref()).or_default().push(p);
    }
    for v in kids.values_mut() {
        v.sort_by(|a, b| (&a.position, &a.id).cmp(&(&b.position, &b.id)));
    }
    let mut out = String::new();
    let mut n = 0;
    fn walk<'a>(
        lang: Lang,
        kids: &HashMap<Option<&'a str>, Vec<&'a Page>>,
        at: Option<&'a str>,
        depth: usize,
        out: &mut String,
        n: &mut usize,
    ) {
        for p in kids.get(&at).map(Vec::as_slice).unwrap_or(&[]) {
            *n += 1;
            let icon = p
                .icon
                .as_deref()
                .map(|i| format!("{i} "))
                .unwrap_or_default();
            let kind = if p.kind == "database" {
                tr!(lang, "［数据库］")
            } else {
                String::new()
            };
            out.push_str(&tr!(
                lang,
                "{indent}- {icon}{kind}{title} （page_id: {id}，更新于 {time}）\n",
                indent = "  ".repeat(depth),
                icon = icon,
                kind = kind,
                title = title(lang, p),
                id = p.id,
                time = fmt_time(p.updated_at)
            ));
            walk(lang, kids, Some(p.id.as_str()), depth + 1, out, n);
        }
    }
    if let Some(r) = root {
        match pages.iter().find(|p| p.id == r) {
            Some(p) => out.push_str(&tr!(
                lang,
                "《{title}》下面的页面：\n",
                title = title(lang, p)
            )),
            None => return tr!(lang, "找不到页面 {id}。", id = r),
        }
    }
    walk(lang, &kids, root, 0, &mut out, &mut n);
    if n == 0 {
        return if root.is_some() {
            out + &tr!(lang, "（没有子页面）")
        } else {
            tr!(lang, "工作区里还没有页面。")
        };
    }
    out + &tr!(lang, "\n共 {n} 个页面。", n = n)
}

pub fn render_page(
    lang: Lang,
    page: &Page,
    doc: &Node,
    base: &str,
    threads: &[ThreadView],
    from: Option<&str>,
    max: usize,
) -> Result<String, Error> {
    let blocks = &doc.content;
    let start = match from {
        None => 0,
        Some(f) => blocks
            .iter()
            .position(|b| b.attr_str("id") == Some(f))
            .ok_or_else(|| {
                Error::NotFound(tr!(Lang::Zh, "页面里没有块 ⟦{id}⟧（顶层块才行）", id = f))
            })?,
    };
    let end = (start + max).min(blocks.len());
    let mut s = tr!(
        lang,
        "# {title}\npage_id: {id} · 更新于 {time} · 共 {n} 块\nbase: {base}\n",
        title = title(lang, page),
        id = page.id,
        time = fmt_time(page.updated_at),
        n = blocks.len(),
        base = base
    );
    if blocks.is_empty() {
        s.push_str(&tr!(lang, "\n（正文是空的）\n"));
    }
    for b in &blocks[start..end] {
        let id = b.attr_str("id").unwrap_or("?");
        s.push_str(&format!(
            "\n⟦{id}⟧\n{}\n",
            oad_markdown::serialize_block(b).trim_end()
        ));
    }
    if start > 0 {
        s.push_str(&tr!(lang, "\n（前面还有 {n} 块没显示）", n = start));
    }
    if end < blocks.len() {
        let next = blocks[end].attr_str("id").unwrap_or("?");
        s.push_str(&tr!(
            lang,
            "\n（后面还有 {n} 块。接着读：read_page(page_id: \"{id}\", from_block: \"{next}\")）\n",
            n = blocks.len() - end,
            id = page.id,
            next = next
        ));
    }
    if !threads.is_empty() {
        s.push_str(&tr!(
            lang,
            "\n没解决的评论（{n} 串，详细用 list_comments）：\n",
            n = threads.len()
        ));
        for t in threads {
            let first = t.comments.iter().find(|c| c.deleted_at.is_none());
            let at = anchor_text(lang, t);
            let (by, body) = first
                .map(|c| {
                    (
                        who(lang, c.author.kind, &c.author.name),
                        excerpt(&c.body, 80),
                    )
                })
                .unwrap_or_default();
            let more = t.comments.len().saturating_sub(1);
            s.push_str(&tr!(
                lang,
                "- thread_id: {id} · {at} · {by}：{body}{more}\n",
                id = t.id,
                at = at,
                by = by,
                body = body,
                more = if more > 0 {
                    tr!(lang, "（另有 {n} 条回复）", n = more)
                } else {
                    String::new()
                }
            ));
        }
    }
    Ok(s)
}

fn anchor_text(lang: Lang, t: &ThreadView) -> String {
    match &t.anchor {
        Some(a) => {
            let mut s = match &a.block_id {
                Some(b) => tr!(lang, "挂在 ⟦{b}⟧", b = b),
                None => tr!(lang, "挂在页面上"),
            };
            if let Some(q) = a.quote.as_deref().filter(|q| !q.is_empty()) {
                s.push_str(&quote(lang, &excerpt(q, 60)));
            }
            s
        }
        None => tr!(lang, "对整页"),
    }
}

pub fn render_edit(
    lang: Lang,
    kinds: &[OpKind],
    refs: &[Option<String>],
    results: &[Vec<String>],
    base: &str,
) -> String {
    let ids = |v: &[String]| {
        v.iter()
            .map(|i| format!("⟦{i}⟧"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut s = tr!(lang, "已改好（{n} 步）：\n", n = results.len());
    for (i, ((k, r), got)) in kinds.iter().zip(refs).zip(results).enumerate() {
        let b = r.as_deref().unwrap_or("?");
        let line = match k {
            OpKind::Replace => tr!(lang, "替换 ⟦{b}⟧ → {ids}", b = b, ids = ids(got)),
            OpKind::InsertAfter => tr!(lang, "在 ⟦{b}⟧ 后面插入 {ids}", b = b, ids = ids(got)),
            OpKind::InsertBefore => tr!(lang, "在 ⟦{b}⟧ 前面插入 {ids}", b = b, ids = ids(got)),
            OpKind::Append => tr!(lang, "在最后加上 {ids}", ids = ids(got)),
            OpKind::Delete => tr!(lang, "删掉 ⟦{b}⟧", b = b),
        };
        s.push_str(&format!("{}. {line}\n", i + 1));
    }
    s.push_str(&tr!(lang, "新的 base: {base}", base = base));
    s
}

pub fn render_threads(lang: Lang, page: &Page, threads: &[ThreadView], all: bool) -> String {
    let tt = title(lang, page);
    if threads.is_empty() {
        return if all {
            tr!(lang, "《{title}》没有评论。", title = tt)
        } else {
            tr!(lang, "《{title}》没有没解决的评论。", title = tt)
        };
    }
    let mut s = if all {
        tr!(
            lang,
            "《{title}》的评论（{n} 串）：\n",
            title = tt,
            n = threads.len()
        )
    } else {
        tr!(
            lang,
            "《{title}》的评论（没解决的 {n} 串）：\n",
            title = tt,
            n = threads.len()
        )
    };
    for t in threads {
        let state = if t.resolved_at.is_some() {
            tr!(lang, "已解决")
        } else {
            tr!(lang, "没解决")
        };
        s.push_str(&tr!(
            lang,
            "\n【thread_id: {id}】{at} · {state}\n",
            id = t.id,
            at = anchor_text(lang, t),
            state = state
        ));
        for c in &t.comments {
            if c.deleted_at.is_some() {
                s.push_str(&tr!(lang, "  - （已删除）\n"));
                continue;
            }
            s.push_str(&tr!(
                lang,
                "  - {who}（{time}）：{body}\n",
                who = who(lang, c.author.kind, &c.author.name),
                time = fmt_time(c.created_at),
                body = c.body.replace('\n', "\n    ")
            ));
        }
    }
    s
}

pub fn render_notes(lang: Lang, notes: &[Value]) -> String {
    if notes.is_empty() {
        return tr!(lang, "没有新通知。");
    }
    let mut s = tr!(lang, "{n} 条通知：\n", n = notes.len());
    for n in notes {
        let id = n["id"].as_i64().unwrap_or_default();
        let to = n["agent"]["name"].as_str().unwrap_or("");
        s.push_str(&tr!(
            lang,
            "\n【通知 {id}】（给 {to}，{time}）\n",
            id = id,
            to = to,
            time = fmt_time(n["created_at"].as_i64().unwrap_or_default())
        ));
        s.push_str(n["prompt"].as_str().unwrap_or(""));
        s.push('\n');
    }
    let last = notes
        .last()
        .and_then(|n| n["id"].as_i64())
        .unwrap_or_default();
    s.push_str(&tr!(
        lang,
        "\n处理完后调用 my_notifications(after: {last}) 把这些标成已处理并取新的。",
        last = last
    ));
    s
}

/// 一格放进 Markdown 表格：去掉换行，`|` 转义，太长的截断。
fn md_cell(s: &str) -> String {
    let one: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut t: String = one.chars().take(120).collect();
    if one.chars().count() > 120 {
        t.push('…');
    }
    t.replace('|', "\\|")
}

/// db_query 的结果：Markdown 表格，第一列是 row_id，末尾是总数和翻页提示。
pub fn render_db_table(lang: Lang, page: &Page, s: &oad_core::db::Shown) -> String {
    let mut out = tr!(
        lang,
        "《{title}》（db_id: {id}）\n\n",
        title = title(lang, page),
        id = page.id
    );
    if s.rows.is_empty() {
        out.push_str(&if s.total == 0 {
            tr!(lang, "没有符合条件的行。\n")
        } else {
            tr!(lang, "这一段没有行。\n")
        });
        out.push_str(&tr!(lang, "\n共 {n} 行\n", n = s.total));
        return out;
    }
    out.push_str("| row_id |");
    for c in &s.columns {
        out.push_str(&format!(" {} |", md_cell(c)));
    }
    out.push_str("\n|---|");
    for _ in &s.columns {
        out.push_str("---|");
    }
    out.push('\n');
    for r in &s.rows {
        out.push_str(&format!("| {} |", r.id));
        for c in &r.cells {
            out.push_str(&format!(" {} |", md_cell(c)));
        }
        out.push('\n');
    }
    let end = s.offset + s.rows.len();
    out.push_str(&tr!(
        lang,
        "\n共 {total} 行，这里是第 {from}–{to} 行",
        total = s.total,
        from = s.offset + 1,
        to = end
    ));
    if end < s.total {
        out.push_str(&tr!(lang, "。下一段：offset: {end}", end = end));
    }
    out.push('\n');
    out
}

/// 行页面的属性（read_page 读行时放在最前面）。
pub fn render_row_props(lang: Lang, db: &Page, props: &[(String, String)]) -> String {
    let mut out = tr!(
        lang,
        "这是数据库《{title}》（db_id: {id}）里的一行。属性：\n",
        title = title(lang, db),
        id = db.id
    );
    if props.is_empty() {
        out.push_str(&tr!(lang, "（都是空的）\n"));
    }
    for (k, v) in props {
        out.push_str(&kv_line(lang, k, v));
    }
    out.push_str(&tr!(lang, "改属性用 db_update；下面是这一行的正文。\n\n"));
    out
}

/// 历史版本列表。changes[i] 是第 i 版比它前一版改了的块（最老的一版没有）。
pub fn render_history(
    lang: Lang,
    page: &Page,
    versions: &[oad_core::VersionMeta],
    changes: &[Option<Vec<oad_core::history::BlockDiff>>],
) -> String {
    let mut out = tr!(
        lang,
        "《{title}》（page_id: {id}）的历史，新的在前：\n",
        title = title(lang, page),
        id = page.id
    );
    if versions.is_empty() {
        out.push_str(&tr!(lang, "还没有存过版本。\n"));
        return out;
    }
    for (i, v) in versions.iter().enumerate() {
        let kind = tr!(
            lang,
            match v.kind.as_str() {
                "agent" => "Agent 写入",
                "restore" => "恢复",
                "manual" => "手动",
                _ => "自动",
            }
        );
        let who: Vec<&str> = v.authors.iter().map(|a| a.name.as_str()).collect();
        out.push_str(&format!(
            "- #{} · {} · {kind}",
            v.id,
            fmt_time(v.created_at)
        ));
        if let Some(l) = &v.label {
            out.push_str(&tr!(lang, " · 「{label}」", label = l));
        }
        if !who.is_empty() {
            out.push_str(&format!(" · {}", who.join(list_sep(lang))));
        }
        if let Some(Some(d)) = changes.get(i) {
            let n = |op: &str| d.iter().filter(|x| x.op == op).count();
            let (a, c, r) = (n("added"), n("changed"), n("removed"));
            if a + c + r > 0 {
                out.push_str(&tr!(
                    lang,
                    "（加 {a} 块、改 {c} 块、删 {r} 块）",
                    a = a,
                    c = c,
                    r = r
                ));
            }
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn time() {
        assert_eq!(super::fmt_time(0), "1970-01-01 00:00 UTC");
        assert_eq!(super::fmt_time(1_763_450_000_000), "2025-11-18 07:13 UTC");
    }
}
