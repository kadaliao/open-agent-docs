//! 日期：都按“当地时间”的分钟数算（1970-01-01 00:00 起），不带时区；时区只在时间戳 ↔ 当地时间时用（[`Clock::tz`]）。
//! 存的样子是 ISO：`2026-11-15` 或 `2026-11-15T14:00`。

/// 现在几点、哪个时区。引擎不碰系统时钟（WASM 里由网页每次调用前给），服务器自己设。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clock {
    /// 毫秒时间戳
    pub now_ms: i64,
    /// 当地时间比 UTC 快几分钟（北京时间 480）
    pub tz: i32,
}

impl Clock {
    pub fn new(now_ms: i64, tz: i32) -> Clock {
        Clock { now_ms, tz }
    }

    /// 本机现在（UTC）。WASM 里没有系统时钟，用不了。
    #[cfg(not(target_arch = "wasm32"))]
    pub fn system() -> Clock {
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        Clock { now_ms: ms, tz: 0 }
    }

    /// 现在（当地时间，分钟）。
    pub fn now(&self) -> DateTime {
        DateTime {
            min: self.local_min(self.now_ms),
            time: true,
        }
    }

    pub fn today(&self) -> i64 {
        self.now().day()
    }

    /// 时间戳（毫秒）→ 当地时间的分钟数。
    pub fn local_min(&self, ms: i64) -> i64 {
        ms.div_euclid(60_000) + self.tz as i64
    }

    /// 当地时间的分钟数 → 时间戳（毫秒）。
    pub fn to_ms(&self, min: i64) -> i64 {
        (min - self.tz as i64) * 60_000
    }
}

impl Default for Clock {
    /// 本机现在（UTC）；WASM 里没有系统时钟，是 0（由调用方设）。
    fn default() -> Clock {
        #[cfg(not(target_arch = "wasm32"))]
        return Clock::system();
        #[cfg(target_arch = "wasm32")]
        return Clock { now_ms: 0, tz: 0 };
    }
}

/// 一个时刻（当地时间的分钟数）；`time` 是有没有写到几点（没有就只显示日期）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DateTime {
    pub min: i64,
    pub time: bool,
}

impl DateTime {
    pub fn from_day(day: i64) -> DateTime {
        DateTime {
            min: day * 1440,
            time: false,
        }
    }

    pub fn ymd(y: i64, m: u32, d: u32) -> DateTime {
        DateTime::from_day(days_from_civil(y, m, d))
    }

    /// 1970-01-01 起第几天。
    pub fn day(&self) -> i64 {
        self.min.div_euclid(1440)
    }

    /// 当天的第几分钟。
    pub fn minute_of_day(&self) -> i64 {
        self.min.rem_euclid(1440)
    }

    pub fn parts(&self) -> (i64, u32, u32, u32, u32) {
        let (y, m, d) = civil_from_days(self.day());
        let mm = self.minute_of_day();
        (y, m, d, (mm / 60) as u32, (mm % 60) as u32)
    }

    /// 显示时带不带几点：写了时间，或者不是零点。
    pub fn shows_time(&self) -> bool {
        self.time || self.minute_of_day() != 0
    }

    /// 存的样子：`2026-11-15` / `2026-11-15T14:00`。
    pub fn iso(&self) -> String {
        let (y, m, d, h, mi) = self.parts();
        if self.shows_time() {
            format!("{}T{h:02}:{mi:02}", ymd_str(y, m, d))
        } else {
            ymd_str(y, m, d)
        }
    }

    /// 公式里转成文字的样子：`2026-11-15` / `2026-11-15 14:00`。
    pub fn text(&self) -> String {
        let (y, m, d, h, mi) = self.parts();
        if self.shows_time() {
            format!("{} {h:02}:{mi:02}", ymd_str(y, m, d))
        } else {
            ymd_str(y, m, d)
        }
    }

    /// 加几个月（月底对齐：1 月 31 日加一个月是 2 月 28 / 29 日）。
    pub fn add_months(&self, n: i64) -> DateTime {
        let (y, m, d) = civil_from_days(self.day());
        let total = y * 12 + (m as i64 - 1) + n;
        let (ny, nm) = (total.div_euclid(12), (total.rem_euclid(12) + 1) as u32);
        let nd = d.min(days_in_month(ny, nm));
        DateTime {
            min: days_from_civil(ny, nm, nd) * 1440 + self.minute_of_day(),
            time: self.time,
        }
    }
}

fn ymd_str(y: i64, m: u32, d: u32) -> String {
    if (0..=9999).contains(&y) {
        format!("{y:04}-{m:02}-{d:02}")
    } else {
        format!("{y}-{m:02}-{d:02}")
    }
}

/// 年月日 → 1970-01-01 起第几天（公历，Howard Hinnant 的算法）。
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// 第几天 → 年月日。
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn is_leap(y: i64) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}

pub fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if is_leap(y) => 29,
        _ => 28,
    }
}

/// 星期几：周一 1 … 周日 7。
pub fn weekday(day: i64) -> u32 {
    ((day + 3).rem_euclid(7) + 1) as u32 // 1970-01-01 是周四
}

/// ISO 周数（第几周）和那一周属于哪年。
pub fn iso_week(day: i64) -> (i64, u32) {
    let thursday = day - weekday(day) as i64 + 4;
    let (y, _, _) = civil_from_days(thursday);
    let first = days_from_civil(y, 1, 1);
    (y, ((thursday - first) / 7 + 1) as u32)
}

/// 那一周的周一。
pub fn week_start(day: i64) -> i64 {
    day - weekday(day) as i64 + 1
}

/// 那个月的第一天。
pub fn month_start(day: i64) -> i64 {
    let (y, m, _) = civil_from_days(day);
    days_from_civil(y, m, 1)
}

// ---------- 认日期 ----------

struct Cur<'a> {
    s: &'a [char],
    i: usize,
}

impl Cur<'_> {
    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }
    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn digits(&mut self, min: usize, max: usize) -> Option<i64> {
        let st = self.i;
        while self.i - st < max && self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.i += 1;
        }
        if self.i - st < min {
            self.i = st;
            return None;
        }
        self.s[st..self.i].iter().collect::<String>().parse().ok()
    }
    fn spaces(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.i += 1;
        }
    }
    fn done(&self) -> bool {
        self.i >= self.s.len()
    }
}

/// 认一个日期：`2026-11-15`、`2026/11/15`、`2026.11.15`、`2026年11月15日`，可以带时间（`14:00`、`14:00:30`、`T14:00Z`、
/// `+08:00`、`14点`、`14点30分`）；`today / 今天`、`tomorrow / 明天`、`yesterday / 昨天`、`后天`、`前天`、`now / 现在`。
/// 带时区的换成当地时间。认不出返回 None。
pub fn parse(s: &str, clock: &Clock) -> Option<DateTime> {
    let s = s.trim();
    let low = s.to_lowercase();
    let rel = match low.as_str() {
        "today" | "今天" | "今日" => Some(0),
        "tomorrow" | "明天" | "明日" => Some(1),
        "yesterday" | "昨天" | "昨日" => Some(-1),
        "后天" => Some(2),
        "前天" => Some(-2),
        "now" | "现在" => return Some(clock.now()),
        _ => None,
    };
    if let Some(n) = rel {
        return Some(DateTime::from_day(clock.today() + n));
    }
    let chars: Vec<char> = s.chars().collect();
    let mut c = Cur { s: &chars, i: 0 };
    let neg = c.eat('-');
    let y = c.digits(4, 6)?;
    let y = if neg { -y } else { y };
    let sep = c.peek()?;
    if !matches!(sep, '-' | '/' | '.' | '年') {
        return None;
    }
    c.i += 1;
    let m = c.digits(1, 2)? as u32;
    let sep2 = if sep == '年' { '月' } else { sep };
    if !c.eat(sep2) {
        return None;
    }
    let d = c.digits(1, 2)? as u32;
    if sep == '年' && !c.eat('日') {
        c.eat('号');
    }
    if !(1..=12).contains(&m) || d < 1 || d > days_in_month(y, m) {
        return None;
    }
    let day = days_from_civil(y, m, d);
    let had_t = c.eat('T') || c.eat('t');
    c.spaces();
    if c.done() {
        return if had_t {
            None
        } else {
            Some(DateTime::from_day(day))
        };
    }
    let h = c.digits(1, 2)?;
    let mut mi = 0;
    if c.eat(':') || c.eat('：') {
        mi = c.digits(2, 2)?;
        if c.eat(':') {
            c.digits(2, 2)?;
            if c.eat('.') {
                c.digits(1, 9)?;
            }
        }
    } else if c.eat('点') || c.eat('时') {
        if let Some(x) = c.digits(1, 2) {
            mi = x;
            c.eat('分');
        }
    } else {
        return None;
    }
    if h > 23 || mi > 59 {
        return None;
    }
    let mut min = day * 1440 + h * 60 + mi;
    c.spaces();
    if c.eat('Z') || c.eat('z') {
        min += clock.tz as i64;
    } else if let Some(sign) = c.peek().filter(|x| *x == '+' || *x == '-') {
        c.i += 1;
        let oh = c.digits(2, 2)?;
        c.eat(':');
        let om = c.digits(2, 2).unwrap_or(0);
        let off = oh * 60 + om;
        let off = if sign == '-' { -off } else { off };
        min += clock.tz as i64 - off;
    }
    if !c.done() {
        return None;
    }
    Some(DateTime { min, time: true })
}

/// 存的日期字符串（ISO）→ 时刻；也认 [`parse`] 认的别的写法。
pub fn parse_stored(s: &str, clock: &Clock) -> Option<DateTime> {
    // 最常见的 `YYYY-MM-DD` / `YYYY-MM-DDTHH:MM` 走快路
    let b = s.as_bytes();
    let num = |r: std::ops::Range<usize>| -> Option<i64> {
        let mut n = 0i64;
        for &c in &b[r] {
            if !c.is_ascii_digit() {
                return None;
            }
            n = n * 10 + (c - b'0') as i64;
        }
        Some(n)
    };
    if (b.len() == 10 || b.len() == 16) && b[4] == b'-' && b[7] == b'-' {
        let (y, m, d) = (num(0..4)?, num(5..7)? as u32, num(8..10)? as u32);
        if (1..=12).contains(&m) && d >= 1 && d <= days_in_month(y, m) {
            let day = days_from_civil(y, m, d);
            if b.len() == 10 {
                return Some(DateTime::from_day(day));
            }
            if b[10] == b'T' && b[13] == b':' {
                let (h, mi) = (num(11..13)?, num(14..16)?);
                if h < 24 && mi < 60 {
                    return Some(DateTime {
                        min: day * 1440 + h * 60 + mi,
                        time: true,
                    });
                }
            }
        }
    }
    parse(s, clock)
}

/// 拆日期范围：`甲 → 乙`、`甲 ~ 乙`、`甲 到 乙`、`甲 至 乙`、`甲 - 乙`（两边有空格）、`甲 to 乙`。
pub fn split_range(s: &str) -> (&str, Option<&str>) {
    for sep in [
        "→", "->", "~", "～", "到", "至", " - ", " — ", " to ", " TO ",
    ] {
        if let Some(i) = s.find(sep) {
            return (s[..i].trim(), Some(s[i + sep.len()..].trim()));
        }
    }
    (s.trim(), None)
}

/// 显示一个时刻：`iso`（2026-11-15 14:00，默认）、`zh`（2026年11月15日 14:00）、`slash`（2026/11/15）、`relative`（今天 14:00、明天…，远的按 iso）。
pub fn format(d: &DateTime, style: Option<&str>, clock: &Clock) -> String {
    let (y, m, dd, h, mi) = d.parts();
    let time = if d.shows_time() {
        format!(" {h:02}:{mi:02}")
    } else {
        String::new()
    };
    match style.unwrap_or("iso") {
        "zh" => format!("{y}年{m}月{dd}日{time}"),
        "slash" => format!("{y}/{m:02}/{dd:02}{time}"),
        "relative" => {
            let diff = d.day() - clock.today();
            let word = match diff {
                0 => "今天",
                1 => "明天",
                -1 => "昨天",
                2 => "后天",
                -2 => "前天",
                _ => return format(d, Some("iso"), clock),
            };
            format!("{word}{time}")
        }
        _ => format!("{}{time}", ymd_str(y, m, dd)),
    }
}

/// 公式 formatDate 的格式：YYYY、YY、MM、M、DD、D、HH、H、mm、ss，别的原样。
pub fn format_pattern(d: &DateTime, pat: &str) -> String {
    let (y, m, dd, h, mi) = d.parts();
    let w = weekday(d.day());
    let mut out = String::new();
    let p: Vec<char> = pat.chars().collect();
    let mut i = 0;
    let starts = |i: usize, t: &str| {
        let t: Vec<char> = t.chars().collect();
        p.len() >= i + t.len() && p[i..i + t.len()] == t[..]
    };
    while i < p.len() {
        let (tok, len) = if starts(i, "YYYY") {
            (format!("{y:04}"), 4)
        } else if starts(i, "YY") {
            (format!("{:02}", y.rem_euclid(100)), 2)
        } else if starts(i, "MM") {
            (format!("{m:02}"), 2)
        } else if starts(i, "M") {
            (m.to_string(), 1)
        } else if starts(i, "DD") {
            (format!("{dd:02}"), 2)
        } else if starts(i, "D") {
            (dd.to_string(), 1)
        } else if starts(i, "HH") {
            (format!("{h:02}"), 2)
        } else if starts(i, "H") {
            (h.to_string(), 1)
        } else if starts(i, "mm") {
            (format!("{mi:02}"), 2)
        } else if starts(i, "ss") {
            ("00".to_string(), 2)
        } else if starts(i, "dddd") {
            (
                [
                    "星期一",
                    "星期二",
                    "星期三",
                    "星期四",
                    "星期五",
                    "星期六",
                    "星期日",
                ][w as usize - 1]
                    .to_string(),
                4,
            )
        } else {
            (p[i].to_string(), 1)
        };
        out.push_str(&tok);
        i += len;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLOCK: Clock = Clock {
        // 2026-11-15 10:30 北京时间
        now_ms: 1_763_173_800_000,
        tz: 480,
    };

    #[test]
    fn civil_roundtrip() {
        for z in [-800_000, -1, 0, 1, 59, 60, 365, 20_000, 2_000_000] {
            let (y, m, d) = civil_from_days(z);
            assert_eq!(days_from_civil(y, m, d), z);
        }
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(days_from_civil(2024, 2, 29)), (2024, 2, 29));
        assert_eq!(weekday(days_from_civil(2026, 11, 16)), 1); // 周一
        assert_eq!(weekday(days_from_civil(2026, 11, 15)), 7);
        assert_eq!(iso_week(days_from_civil(2026, 1, 1)), (2026, 1));
        assert_eq!(iso_week(days_from_civil(2027, 1, 1)), (2026, 53));
    }

    #[test]
    fn clock_today() {
        let n = CLOCK.now();
        assert_eq!(n.text(), "2025-11-15 10:30");
    }

    #[test]
    fn parses() {
        let c = Clock::new(0, 480);
        let p = |s: &str| parse(s, &c).map(|d| d.iso());
        assert_eq!(p("2026-11-15").as_deref(), Some("2026-11-15"));
        assert_eq!(p("2026/11/5").as_deref(), Some("2026-11-05"));
        assert_eq!(p("2026.1.5").as_deref(), Some("2026-01-05"));
        assert_eq!(p("2026年11月15日").as_deref(), Some("2026-11-15"));
        assert_eq!(p("2026年11月15号 9点").as_deref(), Some("2026-11-15T09:00"));
        assert_eq!(
            p("2026年11月15日 14点30分").as_deref(),
            Some("2026-11-15T14:30")
        );
        assert_eq!(p("2026-11-15 9:30").as_deref(), Some("2026-11-15T09:30"));
        assert_eq!(
            p("2026-11-15T14:00:59.123").as_deref(),
            Some("2026-11-15T14:00")
        );
        assert_eq!(p("2026-11-15T06:00Z").as_deref(), Some("2026-11-15T14:00"));
        assert_eq!(
            p("2026-11-15T14:00+08:00").as_deref(),
            Some("2026-11-15T14:00")
        );
        assert_eq!(
            p("2026-11-15T00:00-05:00").as_deref(),
            Some("2026-11-15T13:00")
        );
        assert_eq!(p("today").as_deref(), Some("1970-01-01"));
        assert_eq!(p("明天").as_deref(), Some("1970-01-02"));
        for bad in [
            "2026-13-01",
            "2026-02-30",
            "2026-11",
            "下周",
            "2026-11-15 25:00",
            "2026-11-15T",
            "x2026-11-15",
            "2026-11/15",
        ] {
            assert_eq!(p(bad), None, "{bad}");
        }
        assert_eq!(
            parse_stored("2026-11-15T14:00", &c).unwrap(),
            DateTime {
                min: days_from_civil(2026, 11, 15) * 1440 + 840,
                time: true
            }
        );
    }

    #[test]
    fn months() {
        let d = DateTime::ymd(2026, 1, 31);
        assert_eq!(d.add_months(1).iso(), "2026-02-28");
        assert_eq!(d.add_months(13).iso(), "2027-02-28");
        assert_eq!(d.add_months(-2).iso(), "2025-11-30");
        assert_eq!(
            DateTime::ymd(2024, 2, 29).add_months(12).iso(),
            "2025-02-28"
        );
    }

    #[test]
    fn formats() {
        let d = DateTime {
            min: days_from_civil(2026, 11, 5) * 1440 + 9 * 60 + 7,
            time: true,
        };
        assert_eq!(
            format_pattern(&d, "YYYY年M月D日 HH:mm dddd"),
            "2026年11月5日 09:07 星期四"
        );
        assert_eq!(format(&d, Some("zh"), &CLOCK), "2026年11月5日 09:07");
        assert_eq!(format(&d, Some("slash"), &CLOCK), "2026/11/05 09:07");
        assert_eq!(
            split_range("2026-11-15 → 2026-11-20"),
            ("2026-11-15", Some("2026-11-20"))
        );
        assert_eq!(
            split_range("2026-11-15 - 2026-11-20"),
            ("2026-11-15", Some("2026-11-20"))
        );
        assert_eq!(split_range("2026-11-15"), ("2026-11-15", None));
    }
}
