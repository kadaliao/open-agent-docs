//! YouTube：oEmbed 拿标题和频道，i.ytimg.com 拿封面（maxresdefault 不存在就用 hqdefault）。
//! 回退：oEmbed → 网页 meta → 只靠视频编号拼卡片（封面地址是固定格式，总能出一张卡片）。

use url::Url;

use super::{Log, timed};
use crate::error::Result;
use crate::fetch::{Fetcher, RequestOpts};
use crate::model::{Author, Embed, Image, Kind, Provider, Source, Unfurl, YoutubeEmbed};
use crate::providers::generic;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YtRef {
    pub id: String,
    pub start: Option<u32>,
    pub is_short: bool,
    pub is_live: bool,
}

fn valid_id(s: &str) -> bool {
    s.len() == 11 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// `90`、`90s`、`1m30s`、`1h2m3s`、`1m`。
pub fn parse_time(s: &str) -> Option<u32> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(n) = s.parse::<u32>() {
        return Some(n);
    }
    let mut total: u32 = 0;
    let mut num = String::new();
    let mut any = false;
    for c in s.chars() {
        if c.is_ascii_digit() {
            num.push(c);
            continue;
        }
        let n: u32 = num.parse().ok()?;
        num.clear();
        total = total.checked_add(match c {
            'h' | 'H' => n.checked_mul(3600)?,
            'm' | 'M' => n.checked_mul(60)?,
            's' | 'S' => n,
            _ => return None,
        })?;
        any = true;
    }
    if !num.is_empty() {
        // `1m30` 这种结尾没单位的，当秒
        total = total.checked_add(num.parse().ok()?)?;
        any = true;
    }
    any.then_some(total)
}

pub fn parse(url: &Url) -> Option<YtRef> {
    let host = url.host_str()?.to_ascii_lowercase();
    let host = host.trim_start_matches("www.").trim_start_matches("m.").trim_start_matches("music.");
    let segs: Vec<&str> = url.path_segments().map(|s| s.filter(|x| !x.is_empty()).collect()).unwrap_or_default();
    let q = |k: &str| url.query_pairs().find(|(kk, _)| kk == k).map(|(_, v)| v.into_owned());
    let (id, is_short, is_live) = match host {
        "youtu.be" => (segs.first()?.to_string(), false, false),
        "youtube.com" | "youtube-nocookie.com" => match segs.as_slice() {
            ["watch"] => (q("v")?, false, false),
            ["shorts", id, ..] => (id.to_string(), true, false),
            ["live", id, ..] => (id.to_string(), false, true),
            ["embed", id, ..] | ["v", id, ..] | ["e", id, ..] => (id.to_string(), false, false),
            _ => return None,
        },
        _ => return None,
    };
    if !valid_id(&id) {
        return None;
    }
    // t= / start= / #t=
    let frag_t = url.fragment().and_then(|f| f.strip_prefix("t=")).map(str::to_string);
    let start = q("t").or_else(|| q("start")).or(frag_t).and_then(|t| parse_time(&t)).filter(|t| *t > 0);
    Some(YtRef { id, start, is_short, is_live })
}

fn watch_url(r: &YtRef) -> String {
    format!("https://www.youtube.com/watch?v={}", r.id)
}

fn base_card(input: &Url, r: &YtRef, source: Source) -> Unfurl {
    let mut u = Unfurl::new(Kind::Video, Provider::Youtube, source, input.as_str());
    u.canonical_url = Some(if r.is_short { format!("https://www.youtube.com/shorts/{}", r.id) } else { watch_url(r) });
    u.site_name = Some("YouTube".into());
    u.favicon = Some("https://www.youtube.com/favicon.ico".into());
    let mut embed_url = format!("https://www.youtube-nocookie.com/embed/{}", r.id);
    if let Some(s) = r.start {
        embed_url.push_str(&format!("?start={s}"));
    }
    u.embed = Some(Embed::Youtube(YoutubeEmbed {
        video_id: r.id.clone(),
        start_seconds: r.start,
        is_short: r.is_short,
        is_live: r.is_live,
        embed_url,
    }));
    u.thumbnail = Some(Image::sized(format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", r.id), Some(480), Some(360)));
    u
}

/// maxresdefault 不是每个视频都有，HEAD 一下。
async fn best_thumbnail(f: &Fetcher, id: &str) -> Result<Image> {
    let max = Url::parse(&format!("https://i.ytimg.com/vi/{id}/maxresdefault.jpg")).unwrap();
    let r = f.request(&max, RequestOpts::head()).await?;
    if r.status.is_success() {
        Ok(Image::sized(max.as_str(), Some(1280), Some(720)))
    } else {
        Ok(Image::sized(format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg"), Some(480), Some(360)))
    }
}

pub async fn unfurl(f: &Fetcher, input: &Url, r: YtRef, log: &mut Log) -> Result<Unfurl> {
    let mut oembed_url = Url::parse("https://www.youtube.com/oembed?format=json").unwrap();
    oembed_url.query_pairs_mut().append_pair("url", &watch_url(&r));
    // oEmbed 和封面检查并发
    let ((oe, oe_ms), (thumb, th_ms)) =
        tokio::join!(timed("youtube.oembed", generic::fetch_oembed(f, &oembed_url)), timed("youtube.thumbnail", best_thumbnail(f, &r.id)));
    log.record("youtube.oembed", oe_ms, &oe);
    log.record("youtube.thumbnail", th_ms, &thumb);

    if let Ok(o) = oe {
        let mut u = base_card(input, &r, Source::Oembed);
        u.title = o.title;
        u.author = Some(Author {
            name: o.author_name,
            url: o.author_url.clone(),
            handle: o.author_url.as_deref().and_then(handle_from_channel_url),
            avatar: None,
        });
        if let Ok(t) = thumb {
            u.thumbnail = Some(t);
        }
        return Ok(u);
    }

    // oEmbed 失败（私有 / 删除的视频会 401 / 404，或者接口被墙）：试网页 meta
    let watch = Url::parse(&watch_url(&r)).unwrap();
    if let Some(generic::Fetchedpage::Html(p)) = log.step("html", generic::fetch_page(f, &watch, RequestOpts::html())).await {
        let g = p.meta.to_unfurl(input.as_str(), &p.fetched.url);
        if g.title.as_deref().is_some_and(|t| t != "YouTube" && t != "- YouTube") {
            let mut u = base_card(input, &r, Source::Html);
            u.title = g.title;
            u.description = g.description;
            u.author = g.author;
            u.published_at = g.published_at;
            if let Ok(t) = thumb {
                u.thumbnail = Some(t);
            }
            return Ok(u);
        }
    }

    // 最后：只靠视频编号
    let mut u = base_card(input, &r, Source::UrlOnly);
    if let Ok(t) = thumb {
        u.thumbnail = Some(t);
    }
    Ok(u)
}

fn handle_from_channel_url(u: &str) -> Option<String> {
    let rest = u.split("youtube.com/").nth(1)?;
    rest.strip_prefix('@').map(|h| h.trim_end_matches('/').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Option<YtRef> {
        parse(&Url::parse(s).unwrap())
    }

    #[test]
    fn urls() {
        let id = "dQw4w9WgXcQ";
        for s in [
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtube.com/watch?feature=share&v=dQw4w9WgXcQ",
            "https://m.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://music.youtube.com/watch?v=dQw4w9WgXcQ&list=RD",
            "https://youtu.be/dQw4w9WgXcQ",
            "https://www.youtube.com/embed/dQw4w9WgXcQ",
            "https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ?rel=0",
            "https://www.youtube.com/v/dQw4w9WgXcQ",
        ] {
            let r = p(s).unwrap_or_else(|| panic!("{s}"));
            assert_eq!(r.id, id, "{s}");
            assert!(!r.is_short && !r.is_live && r.start.is_none(), "{s}");
        }
        let s = p("https://www.youtube.com/shorts/abcdefghijk?feature=share").unwrap();
        assert!(s.is_short);
        assert_eq!(s.id, "abcdefghijk");
        let l = p("https://www.youtube.com/live/abcdefghij_?si=x").unwrap();
        assert!(l.is_live);
        assert_eq!(p("https://youtu.be/dQw4w9WgXcQ?t=42").unwrap().start, Some(42));
        assert_eq!(p("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=1m30s").unwrap().start, Some(90));
        assert_eq!(p("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=1h2m3s").unwrap().start, Some(3723));
        assert_eq!(p("https://www.youtube.com/embed/dQw4w9WgXcQ?start=75").unwrap().start, Some(75));
        assert_eq!(p("https://www.youtube.com/watch?v=dQw4w9WgXcQ#t=2m").unwrap().start, Some(120));
        assert_eq!(p("https://youtu.be/dQw4w9WgXcQ?t=0").unwrap().start, None);
        // 不是视频
        assert!(p("https://www.youtube.com/@channel").is_none());
        assert!(p("https://www.youtube.com/watch?v=short").is_none());
        assert!(p("https://www.youtube.com/playlist?list=PL123").is_none());
        assert!(p("https://notyoutube.com/watch?v=dQw4w9WgXcQ").is_none());
    }

    #[test]
    fn times() {
        assert_eq!(parse_time("90"), Some(90));
        assert_eq!(parse_time("90s"), Some(90));
        assert_eq!(parse_time("1m30"), Some(90));
        assert_eq!(parse_time("2h"), Some(7200));
        assert_eq!(parse_time("abc"), None);
        assert_eq!(parse_time(""), None);
    }
}
