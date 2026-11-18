//! 通用解析：抓网页 → oEmbed 发现 → Open Graph / Twitter Card / <title> / JSON-LD → 图标。
//! 网址本身是图片 / 视频 / PDF 时直接出文件卡片。

use url::Url;

use super::{Log, js, jstr, ju64};
use crate::error::{Error, Result};
use crate::fetch::{Fetched, Fetcher, RequestOpts};
use crate::html::{self, PageMeta};
use crate::model::{Author, Embed, Image, Kind, Media, MediaKind, OembedEmbed, Provider, Source, Unfurl};

pub struct Page {
    pub fetched: Fetched,
    pub html: String,
    pub meta: PageMeta,
}

pub enum Fetchedpage {
    Html(Box<Page>),
    /// 不是网页（图片、视频、PDF……），已经拼好卡片。
    NotHtml(Box<Unfurl>),
}

pub async fn fetch_page(f: &Fetcher, url: &Url, opts: RequestOpts) -> Result<Fetchedpage> {
    let fetched = f.request(url, opts).await?.ok()?;
    let ct = fetched.content_type().unwrap_or("").to_ascii_lowercase();
    let mime = ct.split(';').next().unwrap_or("").trim().to_string();
    let is_html = mime.is_empty()
        || mime == "text/html"
        || mime == "application/xhtml+xml"
        || (mime == "text/plain" && looks_like_html(&fetched.body));
    if !is_html {
        return Ok(Fetchedpage::NotHtml(Box::new(file_card(url, &fetched.url, &mime))));
    }
    let (text, _enc) = html::decode(&fetched.body, fetched.content_type());
    let meta = PageMeta::parse(&text, &fetched.url);
    Ok(Fetchedpage::Html(Box::new(Page { fetched, html: text, meta })))
}

fn looks_like_html(body: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&body[..body.len().min(512)]).to_ascii_lowercase();
    head.contains("<html") || head.contains("<!doctype html") || head.contains("<head")
}

fn file_card(input: &Url, final_url: &Url, mime: &str) -> Unfurl {
    let (kind, media_kind) = if mime.starts_with("image/") {
        (Kind::Image, Some(MediaKind::Image))
    } else if mime.starts_with("video/") {
        (Kind::File, Some(MediaKind::Video))
    } else if mime.starts_with("audio/") {
        (Kind::File, Some(MediaKind::Audio))
    } else {
        (Kind::File, None)
    };
    let mut u = Unfurl::new(kind, Provider::Generic, Source::Html, input.as_str());
    u.canonical_url = Some(final_url.to_string());
    u.title = final_url.path_segments().and_then(|mut s| s.next_back()).filter(|s| !s.is_empty()).map(percent_decode);
    u.site_name = final_url.host_str().map(|h| h.trim_start_matches("www.").to_string());
    if let Some(mk) = media_kind {
        u.media.push(Media {
            kind: mk,
            url: final_url.to_string(),
            mime: Some(mime.to_string()),
            width: None,
            height: None,
            poster: None,
            duration_ms: None,
            alt: None,
        });
        if mk == MediaKind::Image {
            u.thumbnail = Some(Image::new(final_url.as_str()));
        }
    }
    u.description = Some(mime.to_string()).filter(|m| !m.is_empty());
    u
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("zz"), 16)
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// oEmbed 返回的东西（字段类型各家不统一，宽松地取）。
#[derive(Debug, Default, Clone)]
pub struct Oembed {
    pub kind: String,
    pub title: Option<String>,
    pub author_name: Option<String>,
    pub author_url: Option<String>,
    pub provider_name: Option<String>,
    pub thumbnail: Option<Image>,
    pub html: Option<String>,
    pub url: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

pub async fn fetch_oembed(f: &Fetcher, endpoint: &Url) -> Result<Oembed> {
    let r = f.request(endpoint, RequestOpts { max_body: Some(256 * 1024), ..RequestOpts::json() }).await?.ok()?;
    let v: serde_json::Value = r.json()?;
    if !v.is_object() {
        return Err(Error::Parse("oembed: not an object".into()));
    }
    let u32_of = |k: &str| ju64(&v, &[k]).and_then(|n| u32::try_from(n).ok());
    Ok(Oembed {
        kind: jstr(&v, &["type"]).unwrap_or_else(|| "link".into()),
        title: jstr(&v, &["title"]).map(|s| html::clean(&s)),
        author_name: jstr(&v, &["author_name"]),
        author_url: jstr(&v, &["author_url"]),
        provider_name: jstr(&v, &["provider_name"]),
        thumbnail: jstr(&v, &["thumbnail_url"]).map(|t| Image::sized(t, u32_of("thumbnail_width"), u32_of("thumbnail_height"))),
        html: js(&v, &["html"]).and_then(|h| h.as_str()).map(str::to_string),
        url: jstr(&v, &["url"]),
        width: u32_of("width"),
        height: u32_of("height"),
    })
}

/// 把 oEmbed 的结果补进卡片：oEmbed 的标题 / 作者通常比 OG 准，图只在没有时补。
pub fn merge_oembed(u: &mut Unfurl, o: &Oembed) {
    if o.title.is_some() {
        u.title = o.title.clone();
    }
    if o.author_name.is_some() || o.author_url.is_some() {
        let a = u.author.get_or_insert_with(Author::default);
        if o.author_name.is_some() {
            a.name = o.author_name.clone();
        }
        if o.author_url.is_some() {
            a.url = o.author_url.clone();
        }
    }
    if u.thumbnail.is_none() {
        u.thumbnail = o.thumbnail.clone();
    }
    if u.site_name.is_none() {
        u.site_name = o.provider_name.clone();
    }
    match o.kind.as_str() {
        "photo" => {
            u.kind = Kind::Image;
            if let Some(src) = &o.url {
                u.media.push(Media {
                    kind: MediaKind::Image,
                    url: src.clone(),
                    mime: None,
                    width: o.width,
                    height: o.height,
                    poster: None,
                    duration_ms: None,
                    alt: None,
                });
            }
        }
        "video" | "rich" => {
            if o.kind == "video" {
                u.kind = Kind::Video;
            }
            u.embed = Some(Embed::Oembed(OembedEmbed {
                oembed_type: o.kind.clone(),
                provider_name: o.provider_name.clone(),
                html: o.html.clone(),
                width: o.width,
                height: o.height,
            }));
        }
        _ => {}
    }
}

/// 通用解析的入口。也给专用解析器当最后一级回退用。
pub async fn unfurl(f: &Fetcher, url: &Url, log: &mut Log) -> Result<Unfurl> {
    let page = match log.step("html", fetch_page(f, url, RequestOpts::html())).await {
        Some(p) => p,
        None => return Err(log.all_failed()),
    };
    let page = match page {
        Fetchedpage::NotHtml(u) => return Ok(*u),
        Fetchedpage::Html(p) => p,
    };
    let mut u = page.meta.to_unfurl(url.as_str(), &page.fetched.url);
    if let Some(ep) = page.meta.oembed_json.as_deref().and_then(|s| Url::parse(s).ok())
        && let Some(o) = log.step("oembed.discovered", fetch_oembed(f, &ep)).await
    {
        merge_oembed(&mut u, &o);
        if u.title.is_some() && o.title.is_some() {
            u.source = Source::Oembed;
        }
    }
    if u.title.is_none() && u.description.is_none() && u.thumbnail.is_none() {
        // 网页拿到了，但什么信息都没有（多半是纯 JS 渲染或拦了机器人）。
        return Err(Error::Parse("page has no title/description/image".into()));
    }
    Ok(u)
}
