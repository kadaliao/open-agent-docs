//! 缓存用的网址规范化和有效期。
//!
//! 同一个东西的不同写法算一个：去掉跟踪参数（utm_*、fbclid、si、spm_id_from…）、youtu.be / m.youtube / shorts 统一成
//! `www.youtube.com/watch?v=`、twitter.com / mobile.twitter.com 统一成 x.com、域名小写、去掉 #片段和默认端口、查询参数排序。
use crate::model::{Kind, Provider, Unfurl};
use std::time::Duration;
use url::Url;

/// 跟踪参数（全名）。
const TRACKING: &[&str] = &[
    "fbclid",
    "gclid",
    "dclid",
    "msclkid",
    "igshid",
    "mc_cid",
    "mc_eid",
    "si",
    "feature",
    "ref_src",
    "ref_url",
    "s",
    "spm_id_from",
    "vd_source",
    "share_source",
    "share_medium",
    "share_plat",
    "share_session_id",
    "share_tag",
    "share_from",
    "timestamp",
    "unique_k",
    "from_spmid",
    "_hsenc",
    "_hsmi",
    "mkt_tok",
];

fn is_tracking(host: &str, k: &str) -> bool {
    let k = k.to_ascii_lowercase();
    if k.starts_with("utm_") {
        return true;
    }
    // `s`、`t` 这种短名字只在知道含义的网站上去掉（t 在 YouTube 是开始时间，要留着）
    match k.as_str() {
        "s" | "t" if is_x(host) => true,
        "s" | "feature" | "si" => is_x(host) || is_youtube(host) || host.ends_with("spotify.com"),
        _ => TRACKING.contains(&k.as_str()),
    }
}

fn is_x(h: &str) -> bool {
    matches!(
        h,
        "x.com"
            | "twitter.com"
            | "mobile.twitter.com"
            | "mobile.x.com"
            | "www.x.com"
            | "www.twitter.com"
    )
}

fn is_youtube(h: &str) -> bool {
    h == "youtu.be" || h == "youtube.com" || h.ends_with(".youtube.com")
}

/// 缓存键：规范化后的网址。解析不了的原样（去掉首尾空白）。
pub fn cache_key(input: &str) -> String {
    let Ok(mut u) = crate::normalize(input) else {
        return input.trim().to_string();
    };
    u.set_fragment(None);
    let _ = u.set_username("");
    let _ = u.set_password(None);
    if let Some(h) = u.host_str().map(str::to_ascii_lowercase) {
        let host = if is_x(&h) {
            "x.com".to_string()
        } else if h == "m.youtube.com" || h == "youtube.com" || h == "music.youtube.com" {
            "www.youtube.com".to_string()
        } else if h == "m.bilibili.com" {
            "www.bilibili.com".to_string()
        } else {
            h
        };
        let _ = u.set_host(Some(&host));
    }
    let host = u.host_str().unwrap_or("").to_string();
    // youtu.be/<id> 和 /shorts/<id> → watch?v=<id>
    let mut extra: Vec<(String, String)> = Vec::new();
    if host == "youtu.be" {
        let id = u.path().trim_matches('/').to_string();
        if !id.is_empty() {
            let _ = u.set_host(Some("www.youtube.com"));
            u.set_path("/watch");
            extra.push(("v".into(), id));
        }
    } else if host == "www.youtube.com"
        && let Some(id) = u
            .path()
            .strip_prefix("/shorts/")
            .map(|s| s.trim_matches('/').to_string())
        && !id.is_empty()
    {
        u.set_path("/watch");
        extra.push(("v".into(), id));
    }
    let host = u.host_str().unwrap_or("").to_string();
    let mut q: Vec<(String, String)> = u
        .query_pairs()
        .filter(|(k, _)| !is_tracking(&host, k))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .chain(extra)
        .collect();
    q.sort();
    q.dedup();
    if q.is_empty() {
        u.set_query(None);
    } else {
        u.query_pairs_mut().clear().extend_pairs(q);
    }
    // 路径末尾的 / 统一去掉（根路径除外）
    let path = u.path().to_string();
    if path.len() > 1 && path.ends_with('/') {
        u.set_path(path.trim_end_matches('/'));
    }
    u.to_string()
}

/// 解析成功后多久算过期：帖子、视频 7 天，GitHub 1 小时，别的 1 天。
pub fn ttl(u: &Unfurl) -> Duration {
    const DAY: u64 = 86_400;
    if u.provider == Provider::Github {
        return Duration::from_secs(3600);
    }
    match u.kind {
        Kind::Post | Kind::Video => Duration::from_secs(7 * DAY),
        _ => Duration::from_secs(DAY),
    }
}

/// 连续失败 n 次（n ≥ 1）后多久再试：1 分钟、10 分钟、1 小时。
pub fn failure_backoff(failures: u32) -> Duration {
    match failures {
        0 | 1 => Duration::from_secs(60),
        2 => Duration::from_secs(600),
        _ => Duration::from_secs(3600),
    }
}

/// 网址里的主机名（限速用）。
pub fn host_of(u: &Url) -> String {
    u.host_str().unwrap_or("").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::cache_key;

    #[test]
    fn keys() {
        assert_eq!(
            cache_key("https://youtu.be/dQw4w9WgXcQ?si=abc&t=42"),
            "https://www.youtube.com/watch?t=42&v=dQw4w9WgXcQ"
        );
        assert_eq!(
            cache_key("https://m.youtube.com/watch?v=dQw4w9WgXcQ&t=42&feature=share"),
            "https://www.youtube.com/watch?t=42&v=dQw4w9WgXcQ"
        );
        assert_eq!(
            cache_key("https://www.youtube.com/shorts/abc123"),
            "https://www.youtube.com/watch?v=abc123"
        );
        assert_eq!(
            cache_key("https://twitter.com/jack/status/20?s=20&t=xyz"),
            "https://x.com/jack/status/20"
        );
        assert_eq!(
            cache_key("https://Example.COM:443/a/?utm_source=x&b=2&a=1#frag"),
            "https://example.com/a?a=1&b=2"
        );
        assert_eq!(
            cache_key(
                "https://www.bilibili.com/video/BV1xx411c7mD/?spm_id_from=333.1007&vd_source=abc&p=2"
            ),
            "https://www.bilibili.com/video/BV1xx411c7mD?p=2"
        );
        assert_eq!(cache_key("example.com"), "https://example.com/");
        assert_eq!(
            cache_key("https://a.com/?t=1"),
            "https://a.com/?t=1",
            "别的网站的 t 不动"
        );
    }
}
