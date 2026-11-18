//! 分数索引：同级页面按 position 字符串的字典序排。字符集 0-9A-Za-z（ASCII 有序），末位不为 '0'。
//! 中间插入取中点（算法同 rocicorp/fractional-indexing 的 midpoint），加到最后时只把首位加一（首位已是 z 就往后一位加），键长得很慢。

const DIGITS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

fn digit(c: u8) -> usize {
    DIGITS.iter().position(|&d| d == c).unwrap_or(0)
}

/// a < 结果 < b；None 表示没有边界。
pub fn between(a: Option<&str>, b: Option<&str>) -> String {
    match (a, b) {
        (None, None) => "V".into(),
        (Some(a), None) => after(a),
        (a, Some(b)) => midpoint(a.unwrap_or("").as_bytes(), Some(b.as_bytes())),
    }
}

fn after(a: &str) -> String {
    match a.as_bytes().first() {
        None => "V".into(),
        Some(&c) if digit(c) + 1 < DIGITS.len() => (DIGITS[digit(c) + 1] as char).to_string(),
        Some(_) => format!("z{}", after(&a[1..])),
    }
}

fn midpoint(a: &[u8], b: Option<&[u8]>) -> String {
    if let Some(b) = b {
        let mut n = 0;
        while n < b.len() && a.get(n).copied().unwrap_or(b'0') == b[n] {
            n += 1;
        }
        if n > 0 {
            let head = String::from_utf8_lossy(&b[..n]).into_owned();
            let rest = if n <= a.len() { &a[n..] } else { &[][..] };
            return head + &midpoint(rest, Some(&b[n..]));
        }
    }
    let da = a.first().map(|&c| digit(c)).unwrap_or(0);
    let db = b
        .map(|b| b.first().map(|&c| digit(c)).unwrap_or(0))
        .unwrap_or(DIGITS.len());
    if db - da > 1 {
        let mid = (da + db).div_ceil(2);
        (DIGITS[mid] as char).to_string()
    } else if let Some(b) = b
        && b.len() > 1
    {
        (b[0] as char).to_string()
    } else {
        let rest = if a.is_empty() { &[][..] } else { &a[1..] };
        format!("{}{}", DIGITS[da] as char, midpoint(rest, None))
    }
}

#[cfg(test)]
mod tests {
    use super::between;

    #[test]
    fn ordering() {
        let mut keys = vec![between(None, None)];
        // 一直往后加
        for _ in 0..200 {
            let k = between(Some(keys.last().unwrap()), None);
            assert!(k > *keys.last().unwrap());
            keys.push(k);
        }
        assert!(keys.last().unwrap().len() < 10);
        // 一直往前插
        for _ in 0..200 {
            let k = between(None, Some(&keys[0]));
            assert!(k < keys[0], "{k} {}", keys[0]);
            assert!(!k.ends_with('0'));
            keys.insert(0, k);
        }
        // 反复插在同一对中间
        let (mut a, b) = (keys[10].clone(), keys[11].clone());
        for _ in 0..200 {
            let k = between(Some(&a), Some(&b));
            assert!(a < k && k < b, "{a} {k} {b}");
            assert!(!k.ends_with('0'));
            a = k;
        }
        let (a, mut b) = (keys[20].clone(), keys[21].clone());
        for _ in 0..200 {
            let k = between(Some(&a), Some(&b));
            assert!(a < k && k < b, "{a} {k} {b}");
            b = k;
        }
    }
}
