//! 时区：服务器默认用系统的（`OAD_TZ` 写 IANA 名字可以改，如 `Asia/Shanghai`），按时刻算偏移（有夏令时的地方对）。
//! 以后每个人 / Agent 可以有自己的时区（users.timezone），见 [`crate::Core::timezone_for`]。
use jiff::tz::TimeZone;

/// 服务器的时区：`OAD_TZ`（IANA 名字）> 系统的 > UTC。
pub fn server_tz() -> TimeZone {
    if let Ok(name) = std::env::var("OAD_TZ") {
        let name = name.trim();
        if !name.is_empty() {
            match TimeZone::get(name) {
                Ok(tz) => return tz,
                Err(e) => tracing::warn!("OAD_TZ={name} 不认识（{e}），用系统时区"),
            }
        }
    }
    TimeZone::try_system().unwrap_or(TimeZone::UTC)
}

/// 这个时区在某一时刻（毫秒）比 UTC 快几分钟。
pub fn offset_minutes(tz: &TimeZone, ms: i64) -> i32 {
    match jiff::Timestamp::from_millisecond(ms) {
        Ok(t) => tz.to_offset(t).seconds() / 60,
        Err(_) => 0,
    }
}

/// IANA 名字（没有名字的固定偏移写成 UTC+08:00 这样）。
pub fn name(tz: &TimeZone) -> String {
    match tz.iana_name() {
        Some(n) => n.to_string(),
        None => {
            let m = offset_minutes(tz, jiff::Timestamp::now().as_millisecond());
            let sign = if m < 0 { '-' } else { '+' };
            format!("UTC{sign}{:02}:{:02}", m.abs() / 60, m.abs() % 60)
        }
    }
}

/// 检查用户给的 IANA 名字。
pub fn parse(name: &str) -> Option<TimeZone> {
    TimeZone::get(name.trim()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dst_aware() {
        let ny = parse("America/New_York").unwrap();
        // 2026-01-15 和 2026-07-15 的中午（UTC）
        assert_eq!(offset_minutes(&ny, 1_768_478_400_000), -300);
        assert_eq!(offset_minutes(&ny, 1_784_116_800_000), -240);
        assert_eq!(
            offset_minutes(&parse("Asia/Shanghai").unwrap(), 1_784_116_800_000),
            480
        );
        assert_eq!(name(&ny), "America/New_York");
        assert!(parse("Mars/Base").is_none());
    }
}
