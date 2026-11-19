//! 英文表：变量对得上、不重复、英文里没有中文；src 里直接写的错误信息都翻得到。
use oad_core::i18n::{CORE, DB_ENGINE, Lang, Table, translate};
use std::collections::HashSet;
use std::path::Path;

fn names(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find('{') {
        let after = &rest[i + 1..];
        match after.find('}') {
            Some(j) if j > 0 && after[..j].chars().all(|c| c.is_alphanumeric() || c == '_') => {
                out.push(&after[..j]);
                rest = &after[j + 1..];
            }
            _ => rest = after,
        }
    }
    out.sort_unstable();
    out
}

fn cjk(s: &str) -> bool {
    s.chars().any(|c| matches!(c, '\u{3000}'..='\u{303f}' | '\u{3400}'..='\u{9fff}' | '\u{ff00}'..='\u{ffef}'))
}

fn check(name: &str, t: Table) {
    let mut seen = HashSet::new();
    for (zh, en) in t {
        assert!(seen.insert(*zh), "{name} 重复：{zh}");
        assert_eq!(names(zh), names(en), "{name} 变量对不上：{zh}");
        assert!(!cjk(en), "{name} 英文里有中文：{en}");
        assert!(!zh.contains("}{"), "{name} 两个变量挨着认不出：{zh}");
    }
}

#[test]
fn tables_consistent() {
    check("CORE", CORE);
    check("DB_ENGINE", DB_ENGINE);
}

/// 读一个字符串字面量（`s` 从开头的引号后面开始）；返回内容和读了多少字节。
fn literal(s: &str) -> Option<(String, usize)> {
    let mut out = String::new();
    let mut it = s.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        match c {
            '"' => return Some((out, i + 1)),
            '\\' => match it.next()?.1 {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                '\n' => {
                    while it.peek().is_some_and(|(_, c)| c.is_whitespace()) {
                        it.next();
                    }
                }
                c => out.push(c),
            },
            c => out.push(c),
        }
    }
    None
}

/// format! 的格式串：`{…}` 换成值，`{{` `}}` 是括号本身。
fn fill_format(f: &str, v: &str) -> String {
    let mut out = String::new();
    let mut it = f.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '{' if it.peek() == Some(&'{') => {
                it.next();
                out.push('{');
            }
            '}' if it.peek() == Some(&'}') => {
                it.next();
                out.push('}');
            }
            '{' => {
                for c in it.by_ref() {
                    if c == '}' {
                        break;
                    }
                }
                out.push_str(v);
            }
            c => out.push(c),
        }
    }
    out
}

/// 错误构造：`Error::NotFound(` 这些，和 dbschema 里的 `bad(`。
const PATS: [&str; 7] = [
    "Error::NotFound(",
    "Error::BadRequest(",
    "Error::Forbidden(",
    "Error::Conflict(",
    "Error::Unauthorized(",
    "Error::Gone(",
    "bad(",
];

/// 一个文件里直接写在错误构造里的中文信息（已经把变量填成 x1）。
fn messages(src: &str, pats: &[&str]) -> Vec<String> {
    // 测试模块不算
    let mut code = String::new();
    let mut lines = src.lines().peekable();
    while let Some(l) = lines.next() {
        if l.trim() == "#[cfg(test)]"
            && lines
                .peek()
                .is_some_and(|n| n.trim_start().starts_with("mod "))
        {
            break;
        }
        if !l.trim_start().starts_with("//") {
            code.push_str(l);
            code.push('\n');
        }
    }
    let mut out = Vec::new();
    for pat in pats {
        let mut from = 0;
        while let Some(i) = code[from..].find(pat) {
            let at = from + i;
            let start = at + pat.len();
            from = start;
            if code[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_')
            {
                continue;
            }
            let rest = code[start..].trim_start();
            let (is_fmt, rest) = match rest.strip_prefix("format!(") {
                Some(r) => (true, r.trim_start()),
                None => (false, rest),
            };
            let Some(rest) = rest.strip_prefix('"') else {
                continue;
            };
            let Some((lit, _)) = literal(rest) else {
                continue;
            };
            if !cjk(&lit) {
                continue;
            }
            out.push(if is_fmt { fill_format(&lit, "x1") } else { lit });
        }
    }
    out
}

fn walk(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            walk(&p, files);
        } else if p.extension().is_some_and(|x| x == "rs") {
            files.push(p);
        }
    }
}

#[test]
fn errors_covered() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk(&root, &mut files);
    files.sort();
    let mut missing = Vec::new();
    let mut n = 0;
    for f in &files {
        let src = std::fs::read_to_string(f).unwrap();
        for m in messages(&src, &PATS) {
            n += 1;
            let en = translate(&[CORE, DB_ENGINE], Lang::En, &m);
            if cjk(&en) {
                missing.push(format!(
                    "{}: {m} -> {en}",
                    f.strip_prefix(&root).unwrap().display()
                ));
            }
        }
    }
    assert!(n > 50, "只找到 {n} 条，扫描可能坏了");
    assert!(missing.is_empty(), "没翻到：\n{}", missing.join("\n"));
}

#[test]
fn composite() {
    let t = |m: &str| translate(&[CORE, DB_ENGINE], Lang::En, m);
    assert_eq!(t("第 2 步：找不到块 b1"), "Step 2: Block b1 not found");
    assert_eq!(
        t("第 3 行：「价格」要写数字，不是「很贵」"),
        "Row 3: \"价格\" needs a number, not \"很贵\""
    );
    assert_eq!(
        t("「总价」：公式第 3 个字：没有函数「sun」，是不是 sum？（第 3 个字附近：sun(1)）"),
        "\"总价\": Formula, character 3: No function \"sun\"; did you mean sum? (near character 3: sun(1))"
    );
    assert_eq!(
        t("「编号」写编号，比如 12"),
        "\"编号\": write an ID, e.g. 12"
    );
}
