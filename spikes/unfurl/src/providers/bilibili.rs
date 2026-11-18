//! B 站视频：`x/web-interface/view` 接口 → 网页 meta。b23.tv 短链先跟一次重定向。

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;
use url::Url;

use super::{Log, jstr, ju64, unix_to_rfc3339};
use crate::error::{Error, Result};
use crate::fetch::{Fetcher, RequestOpts};
use crate::html;
use crate::model::{Author, BilibiliEmbed, Embed, Image, Kind, Provider, Source, Unfurl};
use crate::providers::{generic, youtube::parse_time};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BiliId {
    Bv(String),
    Av(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BiliRef {
    pub id: BiliId,
    pub page: u32,
    pub start: Option<u32>,
}

static BV_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(?i:bv)([0-9A-Za-z]{10})$").unwrap());

pub fn is_short_link(url: &Url) -> bool {
    matches!(url.host_str(), Some("b23.tv" | "www.b23.tv" | "bili2233.cn" | "www.bili2233.cn"))
}

pub fn parse(url: &Url) -> Option<BiliRef> {
    let host = url.host_str()?.to_ascii_lowercase();
    let q = |k: &str| url.query_pairs().find(|(kk, _)| kk == k).map(|(_, v)| v.into_owned());
    let id_from = |s: &str| -> Option<BiliId> {
        if let Some(c) = BV_RE.captures(s) {
            return Some(BiliId::Bv(format!("BV{}", &c[1])));
        }
        let lower = s.to_ascii_lowercase();
        lower.strip_prefix("av").and_then(|n| n.parse().ok()).map(BiliId::Av)
    };
    let id = match host.as_str() {
        "www.bilibili.com" | "bilibili.com" | "m.bilibili.com" => {
            let segs: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
            match segs.as_slice() {
                ["video", id, ..] => id_from(id)?,
                _ => return None,
            }
        }
        "player.bilibili.com" => q("bvid").and_then(|b| id_from(&b)).or_else(|| q("aid").and_then(|a| a.parse().ok()).map(BiliId::Av))?,
        _ => return None,
    };
    let page = q("p").or_else(|| q("page")).and_then(|p| p.parse().ok()).filter(|p| *p >= 1).unwrap_or(1);
    let start = q("t").and_then(|t| parse_time(&t)).filter(|t| *t > 0);
    Some(BiliRef { id, page, start })
}

/// 短链：抓一下（只要重定向，正文只读一点点），看最终落在哪。
pub async fn resolve_short(f: &Fetcher, url: &Url) -> Result<Url> {
    let r = f.request(url, RequestOpts { max_body: Some(16 * 1024), ..RequestOpts::html() }).await?;
    Ok(r.url)
}

fn https(u: String) -> String {
    match u.strip_prefix("http://") {
        Some(rest) => format!("https://{rest}"),
        None if u.starts_with("//") => format!("https:{u}"),
        None => u,
    }
}

async fn view_api(f: &Fetcher, r: &BiliRef) -> Result<Value> {
    let q = match &r.id {
        BiliId::Bv(b) => format!("bvid={b}"),
        BiliId::Av(a) => format!("aid={a}"),
    };
    let u = Url::parse(&format!("https://api.bilibili.com/x/web-interface/view?{q}")).unwrap();
    let opts = RequestOpts::json().header(reqwest::header::REFERER, "https://www.bilibili.com/");
    let v: Value = f.request(&u, opts).await?.ok()?.json()?;
    match v.get("code").and_then(|c| c.as_i64()) {
        Some(0) => v.get("data").cloned().ok_or_else(|| Error::Parse("no data".into())),
        Some(-404 | 62002 | 62004) => Err(Error::NotFound),
        Some(-412) => Err(Error::RateLimited),
        c => Err(Error::Parse(format!("bilibili code {c:?}: {}", jstr(&v, &["message"]).unwrap_or_default()))),
    }
}

fn embed_url(bvid: &str, page: u32, start: Option<u32>) -> String {
    let mut s = format!("https://player.bilibili.com/player.html?bvid={bvid}&page={page}&autoplay=0");
    if let Some(t) = start {
        s.push_str(&format!("&t={t}"));
    }
    s
}

fn card_from_api(input: &Url, r: &BiliRef, d: &Value) -> Unfurl {
    let mut u = Unfurl::new(Kind::Video, Provider::Bilibili, Source::Api, input.as_str());
    let bvid = jstr(d, &["bvid"]).unwrap_or_default();
    let pages = d.get("pages").and_then(|p| p.as_array()).cloned().unwrap_or_default();
    let page = pages.iter().find(|p| ju64(p, &["page"]) == Some(r.page as u64));
    u.canonical_url = Some(if r.page > 1 {
        format!("https://www.bilibili.com/video/{bvid}?p={}", r.page)
    } else {
        format!("https://www.bilibili.com/video/{bvid}")
    });
    u.title = jstr(d, &["title"]).map(|t| match page.and_then(|p| jstr(p, &["part"])) {
        Some(part) if pages.len() > 1 => format!("{t} · P{} {part}", r.page),
        _ => t,
    });
    u.description = jstr(d, &["desc"]).filter(|s| s != "-").map(|s| html::truncate(&s, 500));
    u.site_name = Some("哔哩哔哩".into());
    u.favicon = Some("https://www.bilibili.com/favicon.ico".into());
    u.lang = Some("zh-CN".into());
    u.published_at = d.get("pubdate").and_then(|p| p.as_i64()).map(unix_to_rfc3339);
    u.author = Some(Author {
        name: jstr(d, &["owner", "name"]),
        handle: jstr(d, &["owner", "mid"]),
        url: jstr(d, &["owner", "mid"]).map(|m| format!("https://space.bilibili.com/{m}")),
        avatar: jstr(d, &["owner", "face"]).map(https),
    });
    let (mut w, mut h) = (ju64(d, &["dimension", "width"]), ju64(d, &["dimension", "height"]));
    if ju64(d, &["dimension", "rotate"]) == Some(1) {
        std::mem::swap(&mut w, &mut h);
    }
    u.thumbnail = jstr(d, &["pic"]).map(|p| Image::sized(https(p), w.map(|x| x as u32), h.map(|x| x as u32)));
    u.embed = Some(Embed::Bilibili(BilibiliEmbed {
        embed_url: embed_url(&bvid, r.page, r.start),
        bvid,
        aid: ju64(d, &["aid"]),
        cid: page.and_then(|p| ju64(p, &["cid"])).or_else(|| ju64(d, &["cid"])),
        page: r.page,
        start_seconds: r.start,
        duration_seconds: page.and_then(|p| ju64(p, &["duration"])).or_else(|| ju64(d, &["duration"])),
        view_count: ju64(d, &["stat", "view"]),
        like_count: ju64(d, &["stat", "like"]),
        danmaku_count: ju64(d, &["stat", "danmaku"]),
    }));
    u
}

pub async fn unfurl(f: &Fetcher, input: &Url, r: BiliRef, log: &mut Log) -> Result<Unfurl> {
    if let Some(d) = log.step("bilibili.api", view_api(f, &r)).await {
        return Ok(card_from_api(input, &r, &d));
    }
    let page = match &r.id {
        BiliId::Bv(b) => format!("https://www.bilibili.com/video/{b}"),
        BiliId::Av(a) => format!("https://www.bilibili.com/video/av{a}"),
    };
    let page = Url::parse(&page).unwrap();
    if let Some(mut g) = log.step("html", generic::unfurl(f, &page, &mut Log::default())).await {
        g.provider = Provider::Bilibili;
        g.kind = Kind::Video;
        g.url = input.to_string();
        if let (BiliId::Bv(b), None) = (&r.id, &g.embed) {
            g.embed = Some(Embed::Bilibili(BilibiliEmbed {
                bvid: b.clone(),
                aid: None,
                cid: None,
                page: r.page,
                start_seconds: r.start,
                duration_seconds: None,
                view_count: None,
                like_count: None,
                danmaku_count: None,
                embed_url: embed_url(b, r.page, r.start),
            }));
        }
        if let Some(t) = g.thumbnail.as_mut() {
            t.url = https(t.url.clone());
        }
        return Ok(g);
    }
    Err(log.all_failed())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Option<BiliRef> {
        parse(&Url::parse(s).unwrap())
    }

    #[test]
    fn urls() {
        let r = p("https://www.bilibili.com/video/BV1GJ411x7h7/?spm_id_from=333&vd_source=x").unwrap();
        assert_eq!(r, BiliRef { id: BiliId::Bv("BV1GJ411x7h7".into()), page: 1, start: None });
        let r = p("https://m.bilibili.com/video/bv1GJ411x7h7?p=3&t=95").unwrap();
        assert_eq!(r, BiliRef { id: BiliId::Bv("BV1GJ411x7h7".into()), page: 3, start: Some(95) });
        assert_eq!(p("https://www.bilibili.com/video/av170001").unwrap().id, BiliId::Av(170001));
        assert_eq!(p("https://player.bilibili.com/player.html?bvid=BV1GJ411x7h7&page=1").unwrap().id, BiliId::Bv("BV1GJ411x7h7".into()));
        assert!(p("https://www.bilibili.com/bangumi/play/ep1").is_none());
        assert!(p("https://www.bilibili.com/video/BVshort").is_none());
        assert!(is_short_link(&Url::parse("https://b23.tv/abc123").unwrap()));
    }
}
