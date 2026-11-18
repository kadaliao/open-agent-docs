//! 解析结果。这一份结构就是将来 `oad-unfurl` 对外的输出：前端拿它画卡片，服务器把它存进块属性里。
//!
//! 约定：
//! - 所有字段都是“尽力而为”，除了 `kind` / `provider` / `url` / `source` 以外都可能缺。
//! - 时间统一 RFC 3339 字符串（UTC），数字统计用 `u64`。
//! - 图片地址是原站地址；真正上线时要转存到本地 BlobStore（见 README）。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Unfurl {
    /// 这张卡片该怎么画。
    pub kind: Kind,
    /// 哪个网站的专用解析器给出的结果（不认识的网站是 `generic`）。
    pub provider: Provider,
    /// 数据是从哪一级拿到的（专用接口 / oEmbed / 网页 meta / 只靠解析网址）。
    pub source: Source,
    /// 用户给的网址（规范化以后）。
    pub url: String,
    /// 网站自己声明的规范地址，或者跟完重定向以后的地址。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub favicon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<Author>,
    /// 卡片上的主图。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<Image>,
    /// 正文里的图片、视频（X 的配图、OG 里的视频等）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media: Vec<Media>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    /// 各网站特有的数据，前端按 `type` 画专门的卡片 / 播放器。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embed: Option<Embed>,
    /// 每一级尝试的结果，排查问题用（不进块属性也行）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attempts: Vec<Attempt>,
}

impl Unfurl {
    pub fn new(kind: Kind, provider: Provider, source: Source, url: impl Into<String>) -> Self {
        Self {
            kind,
            provider,
            source,
            url: url.into(),
            canonical_url: None,
            title: None,
            description: None,
            site_name: None,
            favicon: None,
            lang: None,
            author: None,
            thumbnail: None,
            media: Vec::new(),
            published_at: None,
            embed: None,
            attempts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// 能播放的视频（YouTube、B 站、og:type=video.*）。
    Video,
    /// 社交网站的一条帖子（X）。
    Post,
    Repo,
    Issue,
    PullRequest,
    /// 文章（og:type=article、公众号文章）。
    Article,
    /// 网址本身就是一张图片。
    Image,
    /// 网址本身是音频 / 视频 / PDF 等文件。
    File,
    /// 其他网页：标题 + 描述 + 图 + 图标的书签卡片。
    Bookmark,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Youtube,
    X,
    Github,
    Bilibili,
    Wechat,
    Generic,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// 网站的数据接口（GitHub REST、B 站 view 接口）。
    Api,
    /// X 的 syndication 接口（非公开）。
    Syndication,
    /// oEmbed（网站提供的或从网页里发现的）。
    Oembed,
    /// 网页里的 Open Graph / Twitter Card / <title> / JSON-LD。
    Html,
    /// 没联网成功，只靠解析网址拼出来的（比如 YouTube 的视频编号和封面）。
    UrlOnly,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Author {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 不带 @ 的用户名。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
}

impl Author {
    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.handle.is_none() && self.url.is_none() && self.avatar.is_none()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Image {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
}

impl Image {
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into(), width: None, height: None, alt: None }
    }
    pub fn sized(url: impl Into<String>, width: Option<u32>, height: Option<u32>) -> Self {
        Self { url: url.into(), width, height, alt: None }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Image,
    Video,
    /// X 的“GIF”其实是循环播放的 mp4。
    Gif,
    Audio,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Media {
    pub kind: MediaKind,
    /// 图片地址；视频是最高码率的 mp4 地址（没有时为 HLS 等其他格式）。
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// 视频的封面图。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poster: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Attempt {
    /// 例如 `x.syndication`、`x.oembed`、`html`。
    pub step: String,
    pub ok: bool,
    pub ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 各网站特有的数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Embed {
    Youtube(YoutubeEmbed),
    X(Box<XPost>),
    GithubRepo(GithubRepo),
    GithubIssue(GithubIssue),
    Bilibili(BilibiliEmbed),
    Wechat(WechatEmbed),
    /// 通用 oEmbed 给的 rich / video HTML。HTML 不可信，前端只能放进 sandbox iframe。
    Oembed(OembedEmbed),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct YoutubeEmbed {
    pub video_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_seconds: Option<u32>,
    pub is_short: bool,
    pub is_live: bool,
    /// youtube-nocookie 播放器地址（已带上开始时间），前端点了封面才加载。
    pub embed_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct XPost {
    pub id: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    pub author: Author,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub author_verified: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub urls: Vec<XUrlEntity>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mentions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hashtags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media: Vec<Media>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub like_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_reply_to: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub possibly_sensitive: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quoted: Option<Box<XPost>>,
    /// 帖子里链接卡片（X 自己抓的 OG），有就带上，省得我们再抓一次。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card: Option<XCard>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct XUrlEntity {
    /// 正文里的 t.co 短链。
    pub url: String,
    pub expanded_url: String,
    pub display_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct XCard {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GithubRepo {
    pub owner: String,
    pub repo: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stars: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forks: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub topics: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pushed_at: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GithubIssue {
    pub owner: String,
    pub repo: String,
    pub number: u64,
    pub is_pull_request: bool,
    /// open / closed / merged / draft
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<GithubLabel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comments: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merged_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additions: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deletions: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed_files: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GithubLabel {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BilibiliEmbed {
    pub bvid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aid: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cid: Option<u64>,
    /// 分 P，从 1 开始。
    pub page: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_seconds: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub like_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub danmaku_count: Option<u64>,
    /// 官方外链播放器地址。
    pub embed_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WechatEmbed {
    /// 公众号名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_avatar: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OembedEmbed {
    /// photo / video / rich / link
    pub oembed_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}
