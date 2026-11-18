//! 把结果写成给模型读的文字。
use crate::OpKind;
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

fn title(p: &Page) -> &str {
    if p.title.is_empty() {
        "（无标题）"
    } else {
        &p.title
    }
}

fn who(kind: PrincipalKind, name: &str) -> String {
    let n = if name.is_empty() {
        "（不知道是谁）"
    } else {
        name
    };
    match kind {
        PrincipalKind::Agent => format!("{n}（Agent）"),
        PrincipalKind::User => n.to_string(),
    }
}

pub fn render_search(q: &str, hits: &[(Page, String)]) -> String {
    if hits.is_empty() {
        return format!("没有找到包含「{q}」的页面。");
    }
    let mut s = format!("找到 {} 个页面：\n", hits.len());
    for (p, snip) in hits {
        s.push_str(&format!("- 《{}》 page_id: {}\n", title(p), p.id));
        if !snip.trim().is_empty() {
            s.push_str(&format!("  {}\n", excerpt(snip, 120)));
        }
    }
    s
}

pub fn render_tree(pages: &[Page], root: Option<&str>) -> String {
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
            out.push_str(&format!(
                "{}- {icon}{} （page_id: {}，更新于 {}）\n",
                "  ".repeat(depth),
                title(p),
                p.id,
                fmt_time(p.updated_at)
            ));
            walk(kids, Some(p.id.as_str()), depth + 1, out, n);
        }
    }
    if let Some(r) = root {
        match pages.iter().find(|p| p.id == r) {
            Some(p) => out.push_str(&format!("《{}》下面的页面：\n", title(p))),
            None => return format!("找不到页面 {r}。"),
        }
    }
    walk(&kids, root, 0, &mut out, &mut n);
    if n == 0 {
        return if root.is_some() {
            format!("{out}（没有子页面）")
        } else {
            "工作区里还没有页面。".into()
        };
    }
    format!("{out}\n共 {n} 个页面。")
}

pub fn render_page(
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
            .ok_or_else(|| Error::NotFound(format!("页面里没有块 ⟦{f}⟧（顶层块才行）")))?,
    };
    let end = (start + max).min(blocks.len());
    let mut s = format!(
        "# {}\npage_id: {} · 更新于 {} · 共 {} 块\nbase: {base}\n",
        title(page),
        page.id,
        fmt_time(page.updated_at),
        blocks.len()
    );
    if blocks.is_empty() {
        s.push_str("\n（正文是空的）\n");
    }
    for b in &blocks[start..end] {
        let id = b.attr_str("id").unwrap_or("?");
        s.push_str(&format!(
            "\n⟦{id}⟧\n{}\n",
            oad_markdown::serialize_block(b).trim_end()
        ));
    }
    if start > 0 {
        s.push_str(&format!("\n（前面还有 {start} 块没显示）"));
    }
    if end < blocks.len() {
        let next = blocks[end].attr_str("id").unwrap_or("?");
        s.push_str(&format!(
            "\n（后面还有 {} 块。接着读：read_page(page_id: \"{}\", from_block: \"{next}\")）\n",
            blocks.len() - end,
            page.id
        ));
    }
    if !threads.is_empty() {
        s.push_str(&format!(
            "\n没解决的评论（{} 串，详细用 list_comments）：\n",
            threads.len()
        ));
        for t in threads {
            let first = t.comments.iter().find(|c| c.deleted_at.is_none());
            let at = anchor_text(t);
            let (by, body) = first
                .map(|c| (who(c.author.kind, &c.author.name), excerpt(&c.body, 80)))
                .unwrap_or_default();
            let more = t.comments.len().saturating_sub(1);
            s.push_str(&format!(
                "- thread_id: {} · {at} · {by}：{body}{}\n",
                t.id,
                if more > 0 {
                    format!("（另有 {more} 条回复）")
                } else {
                    String::new()
                }
            ));
        }
    }
    Ok(s)
}

fn anchor_text(t: &ThreadView) -> String {
    match &t.anchor {
        Some(a) => {
            let mut s = match &a.block_id {
                Some(b) => format!("挂在 ⟦{b}⟧"),
                None => "挂在页面上".into(),
            };
            if let Some(q) = a.quote.as_deref().filter(|q| !q.is_empty()) {
                s.push_str(&format!("「{}」", excerpt(q, 60)));
            }
            s
        }
        None => "对整页".into(),
    }
}

pub fn render_edit(
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
    let mut s = format!("已改好（{} 步）：\n", results.len());
    for (i, ((k, r), got)) in kinds.iter().zip(refs).zip(results).enumerate() {
        let b = r.as_deref().unwrap_or("?");
        let line = match k {
            OpKind::Replace => format!("替换 ⟦{b}⟧ → {}", ids(got)),
            OpKind::InsertAfter => format!("在 ⟦{b}⟧ 后面插入 {}", ids(got)),
            OpKind::InsertBefore => format!("在 ⟦{b}⟧ 前面插入 {}", ids(got)),
            OpKind::Append => format!("在最后加上 {}", ids(got)),
            OpKind::Delete => format!("删掉 ⟦{b}⟧"),
        };
        s.push_str(&format!("{}. {line}\n", i + 1));
    }
    s.push_str(&format!("新的 base: {base}"));
    s
}

pub fn render_threads(page: &Page, threads: &[ThreadView], all: bool) -> String {
    if threads.is_empty() {
        return format!(
            "《{}》没有{}评论。",
            title(page),
            if all { "" } else { "没解决的" }
        );
    }
    let mut s = format!(
        "《{}》的评论（{}{} 串）：\n",
        title(page),
        if all { "" } else { "没解决的 " },
        threads.len()
    );
    for t in threads {
        let state = if t.resolved_at.is_some() {
            "已解决"
        } else {
            "没解决"
        };
        s.push_str(&format!(
            "\n【thread_id: {}】{} · {state}\n",
            t.id,
            anchor_text(t)
        ));
        for c in &t.comments {
            if c.deleted_at.is_some() {
                s.push_str("  - （已删除）\n");
                continue;
            }
            s.push_str(&format!(
                "  - {}（{}）：{}\n",
                who(c.author.kind, &c.author.name),
                fmt_time(c.created_at),
                c.body.replace('\n', "\n    ")
            ));
        }
    }
    s
}

pub fn render_notes(notes: &[Value]) -> String {
    if notes.is_empty() {
        return "没有新通知。".into();
    }
    let mut s = format!("{} 条通知：\n", notes.len());
    for n in notes {
        let id = n["id"].as_i64().unwrap_or_default();
        let to = n["agent"]["name"].as_str().unwrap_or("");
        s.push_str(&format!(
            "\n【通知 {id}】（给 {to}，{}）\n",
            fmt_time(n["created_at"].as_i64().unwrap_or_default())
        ));
        s.push_str(n["prompt"].as_str().unwrap_or(""));
        s.push('\n');
    }
    let last = notes
        .last()
        .and_then(|n| n["id"].as_i64())
        .unwrap_or_default();
    s.push_str(&format!(
        "\n处理完后调用 my_notifications(after: {last}) 把这些标成已处理并取新的。"
    ));
    s
}

#[cfg(test)]
mod tests {
    #[test]
    fn time() {
        assert_eq!(super::fmt_time(0), "1970-01-01 00:00 UTC");
        assert_eq!(super::fmt_time(1_763_450_000_000), "2025-11-18 07:13 UTC");
    }
}
