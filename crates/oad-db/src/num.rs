//! 数字：规整、转文字、按格式显示。

/// 算出来的数规整一下：NaN、无穷是 None；整数原样；别的留 10 位小数（去掉浮点误差，0.1 + 0.2 = 0.3）。
pub fn clean(x: f64) -> Option<f64> {
    if !x.is_finite() {
        return None;
    }
    if x.fract() == 0.0 || x.abs() >= 1e5 {
        // 大数乘 1e10 会丢精度，本来也只剩十来位有效数字
        return Some(x);
    }
    Some((x * 1e10).round() / 1e10)
}

/// 存的样子：整数（±2^53 以内）写成整数。
pub fn to_json(x: f64) -> serde_json::Value {
    if x.fract() == 0.0 && x.abs() < 9_007_199_254_740_992.0 {
        serde_json::Value::from(x as i64)
    } else {
        serde_json::Number::from_f64(x)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null)
    }
}

/// 数转文字（公式里的 toText）：整数不带小数点，别的最多 10 位有效数字（像 `%.10g`）。
pub fn text(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        return format!("{}", x as i64);
    }
    g(x, 10)
}

/// `%.{p}g`
pub fn g(x: f64, p: usize) -> String {
    if x == 0.0 {
        return "0".into();
    }
    if !x.is_finite() {
        return if x.is_nan() {
            "NaN".into()
        } else if x > 0.0 {
            "inf".into()
        } else {
            "-inf".into()
        };
    }
    let e = format!("{:.*e}", p - 1, x);
    let (mant, exp) = e.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    if exp < -4 || exp >= p as i32 {
        let mant = trim_zeros(mant);
        format!("{mant}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs())
    } else {
        let digits = (p as i32 - 1 - exp).max(0) as usize;
        trim_zeros(&format!("{x:.digits$}")).to_string()
    }
}

fn trim_zeros(s: &str) -> &str {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.')
    } else {
        s
    }
}

/// 整数部分加千分位。
fn thousands(s: &str) -> String {
    let (sign, s) = s.strip_prefix('-').map_or(("", s), |r| ("-", r));
    let (int, frac) = s.split_once('.').map_or((s, None), |(a, b)| (a, Some(b)));
    let mut out = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    match frac {
        Some(f) => format!("{sign}{out}.{f}"),
        None => format!("{sign}{out}"),
    }
}

fn fixed(x: f64, digits: u8) -> String {
    let s = format!("{:.*}", digits as usize, x);
    if s.starts_with('-') && s[1..].chars().all(|c| c == '0' || c == '.') {
        s[1..].to_string()
    } else {
        s
    }
}

/// 按属性的格式显示：number（原样）、comma（千分位）、percent（乘 100 加 %）、cny（¥）、usd（$）。
/// 货币不是整数时两位小数；百分比最多两位小数。`precision` 给了就固定几位小数。
pub fn format(x: f64, fmt: Option<&str>, precision: Option<u8>) -> String {
    let plain = |x: f64, max: u8| match precision {
        Some(p) => fixed(x, p),
        None if x.fract() == 0.0 && x.abs() < 1e15 => format!("{}", x as i64),
        None => trim_zeros(&fixed(x, max)).to_string(),
    };
    match fmt.unwrap_or("number") {
        "comma" => thousands(&match precision {
            Some(p) => fixed(x, p),
            None => text(x),
        }),
        "percent" => format!("{}%", plain(x * 100.0, 2)),
        f @ ("cny" | "usd" | "eur") => {
            let sym = match f {
                "cny" => "¥",
                "usd" => "$",
                _ => "€",
            };
            let s = match precision {
                Some(p) => fixed(x.abs(), p),
                None if x.fract() == 0.0 => format!("{}", x.abs() as i64),
                None => fixed(x.abs(), 2),
            };
            let sign = if x < 0.0 && s.chars().any(|c| c.is_ascii_digit() && c != '0') {
                "-"
            } else {
                ""
            };
            format!("{sign}{sym}{}", thousands(&s))
        }
        _ => match precision {
            Some(p) => fixed(x, p),
            None => text(x),
        },
    }
}

/// 认一个数：去掉千分位、空格、货币符号；`50%` 是 0.5。
pub fn parse(s: &str) -> Option<f64> {
    let t: String = s
        .chars()
        .filter(|c| !matches!(c, ',' | '，' | '¥' | '$' | '￥' | '€' | '_') && !c.is_whitespace())
        .collect();
    let (t, pct) = match t.strip_suffix('%').or_else(|| t.strip_suffix('％')) {
        Some(r) => (r.to_string(), true),
        None => (t, false),
    };
    if t.is_empty() || t.eq_ignore_ascii_case("nan") || t.to_lowercase().contains("inf") {
        return None;
    }
    let n: f64 = t.parse().ok()?;
    if !n.is_finite() {
        return None;
    }
    Some(if pct { clean(n / 100.0)? } else { n })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g_like_python() {
        assert_eq!(text(3.0), "3");
        assert_eq!(text(2.5), "2.5");
        assert_eq!(text(1.0 / 3.0), "0.3333333333");
        assert_eq!(text(1e20), "1e+20");
        assert_eq!(text(1.5e-7), "1.5e-07");
        assert_eq!(text(123456789.123), "123456789.1");
        assert_eq!(text(-0.5), "-0.5");
        assert_eq!(clean(0.1 + 0.2), Some(0.3));
        assert_eq!(clean(f64::NAN), None);
    }

    #[test]
    fn formats() {
        assert_eq!(format(1200.0, Some("cny"), None), "¥1,200");
        assert_eq!(format(2.5, Some("cny"), None), "¥2.50");
        assert_eq!(format(-1234.5, Some("usd"), None), "-$1,234.50");
        assert_eq!(format(1234567.891, Some("comma"), None), "1,234,567.891");
        assert_eq!(format(0.5, Some("percent"), None), "50%");
        assert_eq!(format(1.0 / 3.0, Some("percent"), None), "33.33%");
        assert_eq!(format(1.23456, None, Some(2)), "1.23");
        assert_eq!(format(-0.001, None, Some(1)), "0.0");
        assert_eq!(format(10.0, None, None), "10");
    }

    #[test]
    fn parses() {
        assert_eq!(parse("¥1,200"), Some(1200.0));
        assert_eq!(parse(" 3.5 "), Some(3.5));
        assert_eq!(parse("50%"), Some(0.5));
        assert_eq!(parse("1e3"), Some(1000.0));
        assert_eq!(parse("很多"), None);
        assert_eq!(parse(""), None);
        assert_eq!(parse("inf"), None);
    }
}
