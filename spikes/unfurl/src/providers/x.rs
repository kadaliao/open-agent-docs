//! X（Twitter）：syndication 接口（就是 X 官方“嵌入推文”和 react-tweet 用的那个，非公开）
//! → publish.twitter.com oEmbed → 网页 meta → 只靠网址。

use std::sync::LazyLock;

use scraper::{Html, Selector};
use serde_json::Value;
use url::Url;

use super::{Log, js, jstr, ju64};
use crate::error::{Error, Result};
use crate::fetch::{Fetcher, RequestOpts};
use crate::html;
use crate::jsnum::x_syndication_token;
use crate::model::{Author, Embed, Image, Kind, Media, MediaKind, Provider, Source, Unfurl, XCard, XPost, XUrlEntity};
use crate::providers::generic;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XRef {
    pub id: String,
    pub handle: Option<String>,
}

const HOSTS: &[&str] = &[
    "x.com",
    "twitter.com",
    "mobile.twitter.com",
    "mobile.x.com",
    "m.twitter.com",
    // 常见的“修好预览”的镜像域名，用户经常直接贴这种
    "fxtwitter.com",
    "vxtwitter.com",
    "fixupx.com",
    "fixvx.com",
];

pub fn parse(url: &Url) -> Option<XRef> {
    let host = url.host_str()?.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    if !HOSTS.contains(&host) {
        return None;
    }
    let segs: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
    let (handle, id) = match segs.as_slice() {
        ["i", "web", "status", id, ..] | ["i", "status", id, ..] => (None, *id),
        [h, "status" | "statuses", id, ..] => (Some(h.to_string()), *id),
        _ => return None,
    };
    let valid = !id.is_empty() && id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit());
    valid.then(|| XRef { id: id.to_string(), handle })
}

fn syndication_url(id: &str) -> Url {
    let features = [
        "tfw_timeline_list:",
        "tfw_follower_count_sunset:true",
        "tfw_tweet_edit_backend:on",
        "tfw_refsrc_session:on",
        "tfw_fosnr_soft_interventions_enabled:on",
        "tfw_show_birdwatch_pivots_enabled:on",
        "tfw_show_business_verified_badge:on",
        "tfw_duplicate_scribes_to_settings:on",
        "tfw_use_profile_image_shape_enabled:on",
        "tfw_show_blue_verified_badge:on",
        "tfw_legacy_timeline_sunset:true",
        "tfw_show_gov_verified_badge:on",
        "tfw_show_business_affiliate_badge:on",
        "tfw_tweet_edit_frontend:on",
    ]
    .join(";");
    let mut u = Url::parse("https://cdn.syndication.twimg.com/tweet-result").unwrap();
    u.query_pairs_mut()
        .append_pair("id", id)
        .append_pair("lang", "en")
        .append_pair("features", &features)
        .append_pair("token", &x_syndication_token(id));
    u
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&")
}

fn author_of(user: &Value) -> Author {
    let handle = jstr(user, &["screen_name"]);
    Author {
        name: jstr(user, &["name"]),
        url: handle.as_ref().map(|h| format!("https://x.com/{h}")),
        // _normal 是 48px，换成 _bigger（73px）
        avatar: jstr(user, &["profile_image_url_https"]).map(|a| a.replace("_normal.", "_bigger.")),
        handle,
    }
}

fn media_of(t: &Value) -> Vec<Media> {
    let mut out = Vec::new();
    let Some(list) = js(t, &["mediaDetails"]).and_then(|m| m.as_array()) else { return out };
    for m in list {
        let ty = jstr(m, &["type"]).unwrap_or_default();
        let poster = jstr(m, &["media_url_https"]);
        let w = ju64(m, &["original_info", "width"]).map(|x| x as u32);
        let h = ju64(m, &["original_info", "height"]).map(|x| x as u32);
        match ty.as_str() {
            "photo" => {
                if let Some(url) = poster {
                    out.push(Media {
                        kind: MediaKind::Image,
                        url,
                        mime: None,
                        width: w,
                        height: h,
                        poster: None,
                        duration_ms: None,
                        alt: jstr(m, &["ext_alt_text"]),
                    });
                }
            }
            "video" | "animated_gif" => {
                let variants = js(m, &["video_info", "variants"]).and_then(|v| v.as_array()).cloned().unwrap_or_default();
                let best = variants
                    .iter()
                    .filter(|v| jstr(v, &["content_type"]).as_deref() == Some("video/mp4"))
                    .max_by_key(|v| ju64(v, &["bitrate"]).unwrap_or(0))
                    .or_else(|| variants.first());
                if let Some(b) = best.and_then(|b| jstr(b, &["url"]).map(|u| (u, jstr(b, &["content_type"])))) {
                    out.push(Media {
                        kind: if ty == "animated_gif" { MediaKind::Gif } else { MediaKind::Video },
                        url: b.0,
                        mime: b.1,
                        width: w,
                        height: h,
                        poster,
                        duration_ms: ju64(m, &["video_info", "duration_millis"]),
                        alt: jstr(m, &["ext_alt_text"]),
                    });
                }
            }
            _ => {}
        }
    }
    out
}

fn card_of(t: &Value, urls: &[XUrlEntity]) -> Option<XCard> {
    let c = js(t, &["card"])?;
    let bv = |k: &str| jstr(c, &["binding_values", k, "string_value"]);
    let img = ["thumbnail_image_large", "summary_photo_image_large", "photo_image_full_size_large", "thumbnail_image"]
        .iter()
        .find_map(|k| jstr(c, &["binding_values", k, "image_value", "url"]));
    let tco = jstr(c, &["url"]);
    let url = tco.as_ref().and_then(|t| urls.iter().find(|u| &u.url == t).map(|u| u.expanded_url.clone())).or(tco);
    let card =
        XCard { url, title: bv("title"), description: bv("description"), domain: bv("vanity_url").or_else(|| bv("domain")), image: img };
    (card.title.is_some() || card.image.is_some()).then_some(card)
}

/// 把 syndication 的 JSON 变成 XPost。`depth` 控制引用推文只展开一层。
pub fn post_from_syndication(t: &Value, depth: u8) -> Result<XPost> {
    match jstr(t, &["__typename"]).as_deref() {
        Some("TweetTombstone") => return Err(Error::NotFound),
        Some("Tweet") | None => {}
        Some(other) => return Err(Error::Parse(format!("unexpected __typename {other}"))),
    }
    let id = jstr(t, &["id_str"]).ok_or_else(|| Error::Parse("no id_str (tweet not found?)".into()))?;
    // 长推文（note_tweet）的全文在这里
    let raw_text =
        jstr(t, &["note_tweet", "note_tweet_results", "result", "text"]).unwrap_or_else(|| jstr(t, &["text"]).unwrap_or_default());
    let is_note = js(t, &["note_tweet", "note_tweet_results", "result", "text"]).is_some();

    let urls: Vec<XUrlEntity> = js(t, &["entities", "urls"])
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|u| {
                    Some(XUrlEntity {
                        url: jstr(u, &["url"])?,
                        expanded_url: jstr(u, &["expanded_url"])?,
                        display_url: jstr(u, &["display_url"]).unwrap_or_default(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // display_text_range 按 code point 计；范围外是回复对象的 @ 和结尾的媒体短链
    let chars: Vec<char> = raw_text.chars().collect();
    let mut text: String = match (is_note, js(t, &["display_text_range"]).and_then(|r| r.as_array())) {
        (false, Some(r)) if r.len() == 2 => {
            let a = r[0].as_u64().unwrap_or(0) as usize;
            let b = (r[1].as_u64().unwrap_or(chars.len() as u64) as usize).min(chars.len());
            chars[a.min(b)..b].iter().collect()
        }
        _ => raw_text.clone(),
    };
    for u in &urls {
        text = text.replace(&u.url, &u.expanded_url);
    }
    // 媒体的 t.co 短链（范围外的已经切掉，长推文里可能还有）
    if let Some(list) = js(t, &["mediaDetails"]).and_then(|m| m.as_array()) {
        for m in list {
            if let Some(u) = jstr(m, &["url"]) {
                text = text.replace(&u, "");
            }
        }
    }
    let text = unescape(text.trim());

    let user = js(t, &["user"]).cloned().unwrap_or(Value::Null);
    let quoted =
        if depth == 0 { js(t, &["quoted_tweet"]).and_then(|q| post_from_syndication(q, depth + 1).ok()).map(Box::new) } else { None };
    let card = card_of(t, &urls);
    Ok(XPost {
        id,
        text,
        lang: jstr(t, &["lang"]),
        created_at: jstr(t, &["created_at"]),
        author: author_of(&user),
        author_verified: js(&user, &["is_blue_verified"]).and_then(|v| v.as_bool()).unwrap_or(false)
            || js(&user, &["verified"]).and_then(|v| v.as_bool()).unwrap_or(false),
        urls,
        mentions: js(t, &["entities", "user_mentions"])
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|m| jstr(m, &["screen_name"])).collect())
            .unwrap_or_default(),
        hashtags: js(t, &["entities", "hashtags"])
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|m| jstr(m, &["text"])).collect())
            .unwrap_or_default(),
        media: media_of(t),
        like_count: ju64(t, &["favorite_count"]),
        reply_count: ju64(t, &["conversation_count"]),
        in_reply_to: jstr(t, &["in_reply_to_screen_name"]),
        possibly_sensitive: js(t, &["possibly_sensitive"]).and_then(|v| v.as_bool()).unwrap_or(false),
        quoted,
        card,
    })
}

fn card_from_post(input: &Url, p: XPost, source: Source) -> Unfurl {
    let mut u = Unfurl::new(Kind::Post, Provider::X, source, input.as_str());
    let handle = p.author.handle.clone().unwrap_or_else(|| "i".into());
    u.canonical_url = Some(format!("https://x.com/{handle}/status/{}", p.id));
    u.site_name = Some("X".into());
    u.favicon = Some("https://abs.twimg.com/favicons/twitter.3.ico".into());
    u.title = p.author.name.as_ref().map(|n| format!("{n} on X")).or_else(|| p.author.handle.as_ref().map(|h| format!("@{h} on X")));
    u.description = Some(html::truncate(&p.text, 500)).filter(|d| !d.is_empty());
    u.lang = p.lang.clone();
    u.published_at = p.created_at.clone();
    u.author = Some(p.author.clone());
    u.media = p.media.clone();
    u.thumbnail = p
        .media
        .iter()
        .find_map(|m| match m.kind {
            MediaKind::Image => Some(Image { url: m.url.clone(), width: m.width, height: m.height, alt: m.alt.clone() }),
            _ => m.poster.as_ref().map(|pp| Image::sized(pp.clone(), m.width, m.height)),
        })
        .or_else(|| p.card.as_ref().and_then(|c| c.image.clone()).map(Image::new));
    u.embed = Some(Embed::X(Box::new(p)));
    u
}

async fn syndication(f: &Fetcher, r: &XRef) -> Result<XPost> {
    let resp = f.request(&syndication_url(&r.id), RequestOpts::json()).await?.ok()?;
    if resp.body.is_empty() || resp.body.as_slice() == b"{}" {
        return Err(Error::NotFound);
    }
    let v: Value = resp.json()?;
    post_from_syndication(&v, 0)
}

static P_SEL: LazyLock<Selector> = LazyLock::new(|| Selector::parse("blockquote p").unwrap());

/// publish.twitter.com 的 oEmbed：只有正文、作者和日期文字，没有图片和统计。
async fn oembed(f: &Fetcher, r: &XRef) -> Result<XPost> {
    let handle = r.handle.clone().unwrap_or_else(|| "i".into());
    let mut ep = Url::parse("https://publish.twitter.com/oembed?omit_script=1&dnt=true").unwrap();
    ep.query_pairs_mut().append_pair("url", &format!("https://twitter.com/{handle}/status/{}", r.id));
    let o = generic::fetch_oembed(f, &ep).await?;
    let html_src = o.html.clone().ok_or_else(|| Error::Parse("oembed without html".into()))?;
    let (text, lang) = {
        let doc = Html::parse_fragment(&html_src);
        let p = doc.select(&P_SEL).next();
        let text = p.map(|p| p.text().collect::<String>()).unwrap_or_default();
        let lang = p.and_then(|p| p.value().attr("lang")).map(str::to_string);
        (text, lang)
    };
    let author_handle = o.author_url.as_deref().and_then(|u| u.rsplit('/').next()).map(str::to_string).or(r.handle.clone());
    Ok(XPost {
        id: r.id.clone(),
        text: text.trim().to_string(),
        lang,
        // oEmbed 只给 “March 21, 2006” 这种文字，不是标准时间，不填 created_at
        created_at: None,
        author: Author {
            name: o.author_name,
            handle: author_handle.clone(),
            url: author_handle.map(|h| format!("https://x.com/{h}")),
            avatar: None,
        },
        author_verified: false,
        urls: vec![],
        mentions: vec![],
        hashtags: vec![],
        media: vec![],
        like_count: None,
        reply_count: None,
        in_reply_to: None,
        possibly_sensitive: false,
        quoted: None,
        card: None,
    })
}

pub async fn unfurl(f: &Fetcher, input: &Url, r: XRef, log: &mut Log) -> Result<Unfurl> {
    if let Some(p) = log.step("x.syndication", syndication(f, &r)).await {
        return Ok(card_from_post(input, p, Source::Syndication));
    }
    if let Some(p) = log.step("x.oembed", oembed(f, &r)).await {
        return Ok(card_from_post(input, p, Source::Oembed));
    }
    // 两个接口都明确说没有（删了、私密账号、id 不对）：直接报不存在，别再抓网页
    let not_found = |step: &str| log.attempts.iter().any(|a| a.step == step && a.error.as_deref() == Some("not found"));
    if not_found("x.syndication") && not_found("x.oembed") {
        return Err(Error::NotFound);
    }
    let page_url = Url::parse(&format!("https://x.com/{}/status/{}", r.handle.as_deref().unwrap_or("i"), r.id)).unwrap();
    if let Some(mut g) = log.step("html", generic::unfurl(f, &page_url, &mut Log::default())).await
        && g.description.is_some()
    {
        g.provider = Provider::X;
        g.kind = Kind::Post;
        g.url = input.to_string();
        return Ok(g);
    }
    // 只靠网址：至少知道是谁的哪条
    let post = XPost {
        id: r.id.clone(),
        text: String::new(),
        lang: None,
        created_at: None,
        author: Author { handle: r.handle.clone(), url: r.handle.as_ref().map(|h| format!("https://x.com/{h}")), ..Default::default() },
        author_verified: false,
        urls: vec![],
        mentions: vec![],
        hashtags: vec![],
        media: vec![],
        like_count: None,
        reply_count: None,
        in_reply_to: None,
        possibly_sensitive: false,
        quoted: None,
        card: None,
    };
    Ok(card_from_post(input, post, Source::UrlOnly))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Option<XRef> {
        parse(&Url::parse(s).unwrap())
    }

    #[test]
    fn urls() {
        let r = p("https://x.com/jack/status/20").unwrap();
        assert_eq!(r, XRef { id: "20".into(), handle: Some("jack".into()) });
        assert_eq!(p("https://twitter.com/jack/status/20?s=20&t=abc").unwrap().id, "20");
        assert_eq!(p("https://mobile.twitter.com/jack/status/20").unwrap().id, "20");
        assert_eq!(p("https://www.x.com/jack/status/20/photo/1").unwrap().id, "20");
        assert_eq!(p("https://x.com/i/web/status/1683920951807971329").unwrap(), XRef { id: "1683920951807971329".into(), handle: None });
        assert_eq!(p("https://x.com/i/status/1683920951807971329").unwrap().handle, None);
        assert_eq!(p("https://fxtwitter.com/jack/status/20").unwrap().id, "20");
        assert!(p("https://x.com/jack").is_none());
        assert!(p("https://x.com/jack/status/abc").is_none());
        assert!(p("https://x.com/jack/status/123456789012345678901").is_none());
        assert!(p("https://notx.com/jack/status/20").is_none());
    }

    #[test]
    fn syndication_shape() {
        let j: Value = serde_json::from_str(r#"{
          "__typename":"Tweet","id_str":"1","lang":"en","created_at":"2024-01-01T00:00:00.000Z",
          "text":"@bob hello &amp; see https://t.co/abc https://t.co/media",
          "display_text_range":[5,37],
          "entities":{"urls":[{"url":"https://t.co/abc","expanded_url":"https://example.com/x","display_url":"example.com/x"}],
                      "user_mentions":[{"screen_name":"bob"}],"hashtags":[]},
          "user":{"name":"Alice","screen_name":"alice","profile_image_url_https":"https://pbs.twimg.com/p/a_normal.jpg","is_blue_verified":true},
          "mediaDetails":[{"type":"photo","url":"https://t.co/media","media_url_https":"https://pbs.twimg.com/media/1.jpg","original_info":{"width":800,"height":600}},
                          {"type":"video","media_url_https":"https://pbs.twimg.com/poster.jpg","original_info":{"width":1280,"height":720},
                           "video_info":{"duration_millis":5000,"variants":[{"content_type":"application/x-mpegURL","url":"https://v/x.m3u8"},
                             {"bitrate":256000,"content_type":"video/mp4","url":"https://v/low.mp4"},{"bitrate":2176000,"content_type":"video/mp4","url":"https://v/high.mp4"}]}}],
          "favorite_count":42,"conversation_count":3,
          "quoted_tweet":{"__typename":"Tweet","id_str":"2","text":"quoted","user":{"name":"Q","screen_name":"q"}}
        }"#).unwrap();
        let post = post_from_syndication(&j, 0).unwrap();
        assert_eq!(post.text, "hello & see https://example.com/x");
        assert_eq!(post.author.handle.as_deref(), Some("alice"));
        assert_eq!(post.author.avatar.as_deref(), Some("https://pbs.twimg.com/p/a_bigger.jpg"));
        assert!(post.author_verified);
        assert_eq!(post.media.len(), 2);
        assert_eq!(post.media[1].url, "https://v/high.mp4");
        assert_eq!(post.media[1].poster.as_deref(), Some("https://pbs.twimg.com/poster.jpg"));
        assert_eq!(post.like_count, Some(42));
        assert_eq!(post.quoted.as_ref().unwrap().text, "quoted");
        let tomb: Value = serde_json::from_str(r#"{"__typename":"TweetTombstone"}"#).unwrap();
        assert!(matches!(post_from_syndication(&tomb, 0), Err(Error::NotFound)));
    }

    #[test]
    fn syndication_url_has_token() {
        let u = syndication_url("20");
        assert!(u.as_str().contains("token=6dq1a2xwd93"));
        assert!(u.as_str().starts_with("https://cdn.syndication.twimg.com/tweet-result?id=20&"));
    }
}
