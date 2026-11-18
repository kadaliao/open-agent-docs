# 链接解析实验（unfurl）

第 0 阶段的实验：在服务器上把粘贴进来的链接（X、YouTube、GitHub、B 站、公众号、普通网页）变成卡片数据。
结论先说：**能做，而且大部分网站一级就拿到了完整数据**。这里的代码可以直接作为 `oad-unfurl` 的起点。

独立的 cargo 工程，不在仓库的 workspace 里，定完就删。

```sh
cargo run -- 'https://youtu.be/dQw4w9WgXcQ?t=42'          # 打印 JSON 和耗时
cargo run -- -s $(grep -v '^#' live-urls.txt)              # 每个网址一行摘要
OAD_FETCH_PROXY=http://127.0.0.1:7890 cargo run -- <url>   # 走代理
OAD_UNFURL_SKIP=x.syndication,x.oembed cargo run -- <url>  # 跳过某几级，测回退
cargo test                    # 单元测试 + 本机 HTTP 服务测抓取边界，不联网
cargo test -- --ignored       # 联网测试
```

## 输出长什么样

`Unfurl`（`src/model.rs`）是给前端画卡片、存进块属性的数据：

| 字段 | 说明 |
|---|---|
| `kind` | `video` / `post` / `repo` / `issue` / `pull_request` / `article` / `image` / `file` / `bookmark`，决定画哪种卡片 |
| `provider` | `youtube` / `x` / `github` / `bilibili` / `wechat` / `generic` |
| `source` | 数据从哪一级来：`api` / `syndication` / `oembed` / `html` / `url_only` |
| `url`、`canonical_url` | 用户贴的网址；网站声明的规范地址或跟完重定向的地址 |
| `title`、`description`、`site_name`、`favicon`、`lang` | 书签卡片用 |
| `author` | `{name, handle, url, avatar}` |
| `thumbnail` | `{url, width, height, alt}` |
| `media` | 图片 / 视频 / GIF / 音频列表；视频带 `poster`、`duration_ms`，取最高码率的 mp4 |
| `published_at` | RFC 3339 |
| `embed` | 按 `type` 区分的网站专有数据，见下 |
| `attempts` | 每一级尝试的结果和耗时，排查用 |

`embed` 的几种：

- `youtube`：`video_id`、`start_seconds`、`is_short`、`is_live`、`embed_url`（youtube-nocookie，已带开始时间）
- `x`：`id`、`text`（已把 t.co 换成原链接、去掉媒体短链、反转义）、`lang`、`created_at`、`author`、`author_verified`、`urls`、`mentions`、`hashtags`、`media`、`like_count`、`reply_count`、`in_reply_to`、`possibly_sensitive`、`quoted`（引用的帖子，只展开一层）、`card`（X 自己抓好的链接卡片）
- `github_repo`：星数、fork 数、语言、许可证、topics、最近推送时间、是否归档
- `github_issue`：编号、是否 PR、状态（open / closed / merged / draft）、标签、评论数、合并时间、增删行数、分支
- `bilibili`：`bvid`、`aid`、`cid`、分 P、开始时间、时长、播放 / 点赞 / 弹幕数、官方外链播放器地址
- `wechat`：公众号名称、头像
- `oembed`：通用 oEmbed 给的 `video` / `rich` HTML（不可信，前端只能放进 sandbox iframe）

## 各网站怎么取、怎么回退

| 网站 | 第一级 | 第二级 | 第三级 | 最后 |
|---|---|---|---|---|
| YouTube | oEmbed（同时 HEAD 一下 maxresdefault 封面，没有就用 hqdefault） | 网页 meta | — | 只靠视频编号（封面地址固定，总能出卡片） |
| X | syndication 接口 | publish.twitter.com oEmbed | x.com 网页 meta（x.com 对机器人 UA 会给 OG） | 两个接口都说“不存在”就报 `NotFound`；否则只靠网址出卡片 |
| GitHub | REST API（`/pull/N` 404 时再用 issues 接口，GitHub 的 `/pull/N` 可能其实是 issue） | 网页 meta | — | 报错 |
| B 站 | `x/web-interface/view`（带 Referer） | 网页 meta | — | 报错 |
| 公众号 | 网页 meta + 页面脚本变量（`nickname` / `nick_name`、`create_timestamp`、`round_head_img`） | — | — | 报错 |
| 其他 | 网页：oEmbed 发现 → OG → Twitter Card → JSON-LD → `<title>` / description；图标取 `link rel=icon` / `apple-touch-icon`，都没有用 `/favicon.ico` | — | — | 没标题没描述没图就报错 |

网址本身是图片 / 视频 / 音频 / PDF 时，只读响应头不读正文，直接出 `image` / `file` 卡片。b23.tv 短链先跟重定向再分派。

X 的 token 照 react-tweet 的 `((Number(id) / 1e15) * Math.PI).toString(36).replace(/(0+|\.)/g, '')`，
`src/jsnum.rs` 照 V8 的 `DoubleToRadixCString` 复刻了 JS 的 `Number.prototype.toString(36)`，单元测试的期望值是用 `node -e` 算的。

## 实测结果（2026-11-18，这台 Mac mini）

网络情况：环境变量里没有任何代理变量（`HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY` 都没设）；
但本机开着 mihomo 的 TUN 模式（fake-ip，国外域名解析成 `28.0.0.0/8`，这段是公网地址，SSRF 检查放行），
所以下面“直连”一列访问国外网站其实也经过了 TUN。“代理”一列是 `OAD_FETCH_PROXY=http://127.0.0.1:7890`（mihomo 的混合端口）。
**没能测纯国内裸网**（关不了 TUN）。按经验，裸网下 YouTube、X、Vimeo、Spotify 不通，GitHub 时好时坏，B 站、公众号、国内网站正常。

耗时是单次请求（毫秒），同一批网址先直连跑一遍、再走代理跑一遍。

| 网址 | 结果 | 用到哪一级 | 直连 | 代理 | 拿到的东西 |
|---|---|---|---|---|---|
| YouTube 普通视频 | 成功 | oEmbed | 671 | 659 | 标题、频道名和链接、1280×720 封面 |
| youtu.be + `t=10` | 成功 | oEmbed | 408 | 554 | 同上，`start_seconds=10` |
| `t=1m30s` | 成功 | oEmbed | 162 | 202 | `start_seconds=90` |
| Shorts | 成功 | oEmbed | 168 | 206 | `is_short=true` |
| embed + `start=42` | 成功 | oEmbed | 158 | 275 | |
| `/live/` | 成功 | oEmbed | 289 | 324 | `is_live=true` |
| X 纯文字（jack/20） | 成功 | syndication | 497 | 555 | 正文、作者、头像、时间、赞数、回复数 |
| X 图片（Ellen 自拍） | 成功 | syndication | 261 | 173 | 图片原尺寸、赞数 |
| X 图片（twitter.com 域名） | 成功 | syndication | 257 | 277 | |
| X 视频 | 成功 | syndication | 261 | 118 | 1920×1080 mp4 直链、封面、时长 9.3 秒 |
| X 带链接卡片（vercel） | 成功 | syndication | 264 | 259 | `card`：标题、描述、图、已展开的链接 |
| X 不存在的 id | `NotFound` | syndication 和 oEmbed 都 404 | 2041 | 1504 | |
| GitHub 仓库 | 成功 | API | 610 | 457 | 描述、星数、语言、许可证、topics、owner 头像、社交卡片图 |
| GitHub issue（实为 PR） | 成功 | API | 423 | 396 | 自动认成 PR |
| GitHub PR（tokio#6000） | 成功 | API | 762 | 803 | merged、标签、+2/−16、分支 |
| GitHub `/pull/N` 实为 issue | 成功 | API 404 → issues API | 669–778 | 674–734 | |
| B 站 BV | 成功 | API | 222 | 137 | 标题、UP 主和头像、1920×1080 封面、时长、播放 / 点赞 / 弹幕数、发布时间 |
| B 站 av 号 | 成功 | API | 77 | 153 | 多 P 视频标题带上分 P 名 |
| B 站 `p=1&t=30` | 成功 | API | 90 | 74 | 播放器地址带 `t=30` |
| 公众号文章 ×2 | 成功 | 网页 | 1608–1931 | 1063–1196 | 标题、摘要、作者、公众号名、头像、封面、发布时间 |
| blog.rust-lang.org | 成功 | 网页 | 2027 | 1491 | |
| theverge.com | 成功 | 网页 | 1102 | 898 | |
| pconline（GBK，头里没 charset） | 成功 | 网页 | 448 | 412 | 中文标题正确（靠 `<meta charset>` 识别 GBK）。紧接着再抓一次被回了 503 |
| zol（GBK，头里有 charset） | 成功 | 网页 | 593 | 624 | |
| Vimeo | 成功 | 网页 + 发现的 oEmbed | 1578 | 1062 | 直连拿到 oEmbed 播放器 HTML；代理那次网页里没有 oEmbed 链接，只有 OG |
| Spotify 单曲 | 成功 | 网页 + 发现的 oEmbed | 1414 | 2091 | rich 播放器 HTML |
| Hacker News | 成功 | 网页 | 1118 | 1056 | 只有 `<title>` |
| 维基百科条目 | 成功 | 网页 | 1340 | 2407 | JSON-LD 认成 article |
| 图片直链（wikimedia） | 成功 | 只读响应头 | 746 | 672 | `kind=image`（改成不读正文前是 2002 ms） |
| PDF 直链 | 成功 | 只读响应头 | 1047 | 648 | `kind=file` |
| html.spec.whatwg.org（13 MB） | 成功 | 网页，截到 2 MB | 3419 | 3108 | 标题在开头，截断不影响 |
| 36kr.com | **失败** | 网页 | 105 | 143 | 返回的是 JS 验证页，没有任何 meta |

回退路径用 `OAD_UNFURL_SKIP` 单独测过，都能出卡片：
X 跳过 syndication → oEmbed（1.5 s，只有正文和作者，没有图和统计）；再跳过 oEmbed → x.com 网页 OG（1.2–1.8 s，有正文和图）；
YouTube 跳过 oEmbed → 网页 meta（2.2 s，多了描述和发布时间）；GitHub 跳过 API → 网页 OG（1–3 s）；B 站跳过 API → 网页 OG（0.6 s）。

连着请求 20 次 X syndication 没被限流（复用连接后每次约 105 ms）。GitHub 不带 token 每小时 60 次，`/rate_limit` 能看到余量。

### SSRF 实测

直连、走代理两种模式下都拦住了：`http://localhost:8080`、`http://[::ffff:127.0.0.1]/`、`http://0x7f000001/`、`http://2130706433/`、
`http://169.254.169.254/…`、`http://intranet/`，以及解析到内网的域名 `10.0.0.1.sslip.io`、`169-254-169-254.sslip.io`
（直连是自定义解析器拦的，代理模式是预解析拦的）。重定向到内网、`file://`、`localhost`，重定向超过 5 次，正文超 2 MB，超时，
这些用本机起的小 HTTP 服务测（`tests/fetch_local.rs`）。

## 设计说明

**抓取（`src/fetch.rs`、`src/ssrf.rs`）**

- 只允许 http / https，网址里不能带用户名密码。
- 十六进制、十进制、八进制的 IPv4 写法由 `url` 库按 WHATWG 规则规范成普通 IPv4，所以只需判断 `IpAddr`。
- 拒绝：0/8、10/8、100.64/10、127/8、169.254/16、172.16/12、192.0.0/24、文档网段、192.168/16、198.18/15、组播、240/4、广播；
  IPv6 的 `::`、`::1`、fc00::/7、fe80::/10、fec0::/10、ff00::/8、100::/64、2001:db8::/32、2001::/23（含 Teredo），
  以及嵌着 IPv4 的 `::ffff:a.b.c.d`、`::a.b.c.d`、NAT64、6to4（取出 IPv4 再判断）；只放行 2000::/3。
  名字层面拒绝 `localhost`、`*.localhost`、单段主机名、`.local` / `.internal` / `.lan` / `.home.arpa` 等。
- 直连时给 reqwest 装自定义 DNS 解析器，把内网地址过滤掉；reqwest 只会连解析器返回的地址，所以 DNS rebinding 绕不过去。
  网址直接写 IP 时 reqwest 不走解析器，由连接前的检查拦。
- 重定向手动跟（最多 5 跳），每一跳重新检查。一次抓取（含重定向）总共 8 秒，一次解析（含所有回退）总共 20 秒。
- 正文边读边数：JSON 超 2 MB 报错；网页截到 2 MB 继续解析（只需要开头）；不是网页就不读正文。gzip / br 解压后的大小也算在内。
- 不读系统代理环境变量（`no_proxy()`），只认 `OAD_FETCH_PROXY`，免得 `HTTPS_PROXY` 让检查失效。
- `OAD_FETCH_ALLOW_CIDRS` 可以放行个别网段。开发机上 Clash 默认的 fake-ip 段是 `198.18.0.0/15`，会被当成内网全拦掉，
  这时设 `OAD_FETCH_ALLOW_CIDRS=198.18.0.0/15`。这台机器的 mihomo 用的是 `28.0.0.0/8`，不需要设。
- User-Agent：`Mozilla/5.0 (compatible; OpenAgentDocs-Unfurl/0.1; link preview bot)`。带“bot”字样反而有好处：x.com 等网站对机器人 UA 给完整的 OG。

**字符集**：HTTP 头 → BOM → 前 4 KB 里的 `<meta charset>` / `http-equiv` → UTF-8。GB2312、GBK 都按 encoding_rs 的 GBK 解。

**走代理时的局限**：DNS 由代理做，我们没法决定代理连哪个地址。现在的做法是先自己解析一遍，只要有一个地址是内网就拒绝，解析不出来也拒绝。
拦不住的情况：代理那边的 DNS 和我们解析结果不同（分区 DNS、两次解析之间被 rebinding），或者目标在代理所在的内网里。
所以代理本身应当放在没有内网可达的位置。正式版可以改成：自己解析、检查，再让代理 CONNECT 到检查过的 IP（TLS 的 SNI 仍用原域名），这样就没有这个缺口。

## 局限和注意事项

- **X syndication 接口不是公开 API**。它是 X 官方“嵌入推文”小部件和 react-tweet 用的接口，没有文档、没有 SLA，token 算法、`features` 参数随时可能变，
  也可能被限流或要求登录。所以要有 oEmbed 和网页两级回退，而且结果必须存下来（原帖删了卡片也还在）。
  长推文（`note_tweet`）的全文字段按 react-tweet 的结构取了，但这次没找到长推文实测；引用帖子只在单元测试里验证过。
- X oEmbed 只有正文和作者，没有图片、视频、统计，日期是“March 21, 2006”这种文字，没填 `created_at`。
- **图片防盗链**：实测公众号图片（mmbiz.qpic.cn）带别的网站的 Referer 时返回 2 KB 的“未经允许不可引用”占位图，
  B 站图片（hdslb.com）带别的 Referer 直接 403；不带 Referer 都正常。X 的图片不挡。前端至少要 `referrerpolicy="no-referrer"`，正式做法是转存。
- 公众号页面 3.6 MB，发布时间那段变量在 2.9 MB 处（被截掉了），好在 0.5 MB 处还有一份 `create_timestamp`。微信改版就得跟着改；被风控时会给验证页，代码里认了几个关键词。
- 纯 JS 渲染或有机器人验证的网站（36kr）拿不到东西，只能显示成普通链接。可以以后考虑接一个无头浏览器服务，不建议放在主进程里。
- 有的网站连续请求会回 503（pconline 第二次）。正式版要缓存，失败要短时间退避，不能每次打开页面都重抓。
- YouTube 的 maxresdefault 封面要多一次 HEAD 请求（和 oEmbed 并发，不增加总耗时）。
- 通用 oEmbed 的 `html` 是第三方 HTML，不能直接插进页面。

## 给正式版 `oad-unfurl` 的建议

1. **结构**：保留现在的分层——带 SSRF 防护的 `Fetcher`、每个网站一个 provider（`parse(url)` 纯函数 + `unfurl()` 回退链）、
   通用网页解析兜底。`Unfurl` 结构基本可以照搬；`attempts` 只进日志，不进块属性。
2. **缓存**：按规范化后的网址做键（去掉 `utm_*`、`si`、`spm_id_from`、`vd_source` 之类的跟踪参数，YouTube / X / B 站用视频或帖子编号）。
   成功的结果存进数据库，块里存一份快照，不随原站变化。建议 TTL：X 帖子和视频的统计数字 1 天，标题、作者、正文 7 天以上（或永不自动刷新，用户手动刷新）；
   普通网页 7 天；失败结果缓存 10 分钟（不存在 / 被拦的 1 天），免得同一个坏链接反复抓。同一网址并发请求要合并成一次。
3. **图片转存**：封面、头像、帖子图片下载到 BlobStore，卡片里换成本地地址。理由：防盗链（公众号、B 站实测会挡）、原帖删了图也在、
   不把读者的 IP 和 Referer 泄露给第三方、国内访问 twimg / ytimg 不稳定。下载图片走同一个 `Fetcher`（同样的 SSRF 检查），
   限制大小（比如 10 MB）、检查 Content-Type 和图片头、可以顺便缩成缩略图。视频不转存，只存地址和封面。
4. **限流**：对外按域名限并发和速率（比如每个域名 2 个并发、每秒几次），GitHub 按响应头里的 `x-ratelimit-remaining` 退避，
   配置项里留 `OAD_GITHUB_TOKEN`（实验代码已支持）。对内按用户 / Agent 限每分钟解析次数，防止被当成代理去扫别人的网站。
5. **代理**：服务器在国内时，YouTube、X、GitHub 需要 `OAD_FETCH_PROXY`；B 站、公众号最好直连。可以按域名分流（配置一个“走代理”的域名列表），
   并把上面说的“自己解析再让代理 CONNECT 到 IP”做掉。
6. **异步**：粘贴时先插一个“正在解析”的卡片，后台任务解析完再写回 CRDT 文档，不要让粘贴等 2 秒。
7. 以后加的网站：Figma、Loom 网页里有 oEmbed 链接，通用解析已经能拿到；Spotify、Vimeo 实测可用。
   需要单独写的：抖音 / 小红书（反爬重，可能拿不到）、知乎（要登录）。

## 文件

- `src/model.rs`：输出结构
- `src/fetch.rs`、`src/ssrf.rs`：带防护的抓取
- `src/html.rs`：字符集、OG / Twitter Card / JSON-LD / 图标
- `src/jsnum.rs`：JS 数字转 36 进制、X 的 token
- `src/providers/`：各网站
- `tests/fetch_local.rs`：本机 HTTP 服务测重定向、大小、超时；`tests/live.rs`：联网测试（`--ignored`）
- `live-urls.txt`：实测用的网址
