//! 复刻 JavaScript `Number.prototype.toString(radix)` 对非整数的输出（照 V8 的 `DoubleToRadixCString`）。
//! X syndication 接口的 token 依赖它：`((Number(id) / 1e15) * Math.PI).toString(36).replace(/(0+|\.)/g, '')`。

const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";

fn next_double(v: f64) -> f64 {
    // 只用于非负有限数
    if v.is_infinite() {
        return v;
    }
    f64::from_bits(v.to_bits() + 1)
}

/// V8 `Double::Exponent()`：value = significand(53 位整数) × 2^exponent。
fn double_exponent(v: f64) -> i32 {
    let bits = v.to_bits();
    let biased = ((bits >> 52) & 0x7ff) as i32;
    if biased == 0 { 1 - 1075 } else { biased - 1075 }
}

pub fn to_radix_string(value: f64, radix: u32) -> String {
    assert!((2..=36).contains(&radix));
    if value.is_nan() {
        return "NaN".into();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if value == 0.0 {
        return "0".into();
    }
    let radix_f = radix as f64;
    let negative = value < 0.0;
    let value = value.abs();

    let mut integer = value.floor();
    let mut fraction = value - integer;
    // 只算到这个 double 精度范围内的小数位
    let mut delta = 0.5 * (next_double(value) - value);
    delta = delta.max(next_double(0.0));

    let mut frac_digits: Vec<u8> = Vec::new();
    if fraction >= delta {
        loop {
            fraction *= radix_f;
            delta *= radix_f;
            let digit = fraction as usize;
            frac_digits.push(DIGITS[digit]);
            fraction -= digit as f64;
            // 四舍五入到偶数
            if (fraction > 0.5 || (fraction == 0.5 && (digit & 1) == 1)) && fraction + delta > 1.0 {
                // 进位，可能一路进到整数部分
                loop {
                    match frac_digits.pop() {
                        None => {
                            integer += 1.0;
                            break;
                        }
                        Some(c) => {
                            let d = if c > b'9' { (c - b'a' + 10) as u32 } else { (c - b'0') as u32 };
                            if d + 1 < radix {
                                frac_digits.push(DIGITS[(d + 1) as usize]);
                                break;
                            }
                        }
                    }
                }
                break;
            }
            if fraction < delta {
                break;
            }
        }
    }

    // 整数部分；超出精度的低位补 0
    let mut int_digits: Vec<u8> = Vec::new();
    while double_exponent(integer / radix_f) > 0 {
        integer /= radix_f;
        int_digits.push(b'0');
    }
    loop {
        let remainder = integer % radix_f;
        int_digits.push(DIGITS[remainder as usize]);
        integer = (integer - remainder) / radix_f;
        if integer <= 0.0 {
            break;
        }
    }
    int_digits.reverse();

    let mut out = String::with_capacity(int_digits.len() + frac_digits.len() + 2);
    if negative {
        out.push('-');
    }
    out.push_str(std::str::from_utf8(&int_digits).unwrap());
    if !frac_digits.is_empty() {
        out.push('.');
        out.push_str(std::str::from_utf8(&frac_digits).unwrap());
    }
    out
}

/// react-tweet 的 `getToken(id)`。
pub fn x_syndication_token(id: &str) -> String {
    let n: f64 = id.trim().parse().unwrap_or(f64::NAN);
    to_radix_string((n / 1e15) * std::f64::consts::PI, 36).chars().filter(|c| *c != '0' && *c != '.').collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // 期望值用 node 算出：
    // node -e "const id='...';console.log(((Number(id)/1e15)*Math.PI).toString(36))"
    #[test]
    fn radix_matches_node() {
        assert_eq!(to_radix_string((20.0 / 1e15) * std::f64::consts::PI, 36), "0.000000006dq1a2xwd93");
        assert_eq!(to_radix_string((1683920951807971329f64 / 1e15) * std::f64::consts::PI, 36), "42y.6z0v7ufp");
        assert_eq!(to_radix_string((1880000000000000000f64 / 1e15) * std::f64::consts::PI, 36), "4k2.6zo2j29f");
        assert_eq!(to_radix_string((999999999999999999999f64 / 1e15) * std::f64::consts::PI, 36), "1vc2g.nj1vvg");
        assert_eq!(to_radix_string(0.5, 2), "0.1");
        assert_eq!(to_radix_string(255.0, 16), "ff");
        assert_eq!(to_radix_string(-10.25, 2), "-1010.01");
        assert_eq!(to_radix_string(0.1, 3), "0.0022002200220022002200220022002201");
        assert_eq!(to_radix_string(1.0 / 3.0, 36), "0.c");
        assert_eq!(to_radix_string(123.456, 36), "3f.gez4w97ry");
        // 超过 2^53 的整数，低位补 0
        assert_eq!(to_radix_string(2f64.powi(60), 36), "8rc4kbdvss00");
        assert_eq!(to_radix_string(1e21 + 0.5, 36), "5v1j4f4ds7c000");
    }

    #[test]
    fn token_matches_node() {
        for (id, tok) in [
            ("20", "6dq1a2xwd93"),
            ("1683920951807971329", "42y6zv7ufp"),
            ("1880000000000000000", "4k26zo2j29f"),
            ("463440424141459456", "14fxvks611f"),
            ("1234567890123456789", "2zqic77uqyk"),
            ("1", "bhi2ay3f28n"),
        ] {
            assert_eq!(x_syndication_token(id), tok, "id {id}");
        }
    }
}
