# 第 3 阶段约定（富媒体）

在第 1、2 阶段上加。服务器和网页并行做，按这里对接；改约定先改这份文件。

## 验收

1. 空行里贴 X、YouTube、B 站、GitHub、公众号、普通网页的链接：**立刻**出卡片骨架，有缓存 < 300ms 出完整卡片，没缓存一般 < 2s；
   X 是自己画的帖子卡片（正文、作者、图、视频、引用），YouTube / B 站是封面，点了才加载播放器。
2. 贴完弹出小菜单：嵌入 / 书签 / 行内提及 / 普通链接，之后随时能在块菜单里换。
3. 拖进来或粘贴图片、视频、音频、PDF、任意文件：上传，图片先显示本地预览和模糊占位，传完换成服务器地址；图片能拖边改宽度、写图注。
4. 代码块有语法高亮（按需加载），`mermaid` 代码块画成图；公式可视化编辑。
5. 粘贴 Markdown 文本变成对应的块（浏览器里跑 WASM 版 oad-markdown，结果和服务器一致）。
6. 块左边有拖动把手，能拖动排序、点开块菜单（转换类型、复制链接、删除…）。
7. 10 个视频嵌入的页面滚动不掉帧（60fps），打开不比没嵌入的慢多少。
8. Agent 写一行网址就是卡片；服务器在 Agent 写入时就开始解析，网页打开时卡片已经好了。

## 文档结构的变化

- `embed{url, view}`：`view` = `"embed"`（默认，按网站画富卡片 / 播放器）| `"bookmark"`（书签卡片）。**所有外部内容和上传的视频、音频、PDF、附件都是 embed**，
  怎么画由解析结果的 `kind` 决定；上传的文件网址是 `/api/v1/files/<id>/<文件名>`。Markdown 里就是单独一行的网址（和现在一样）。
- 新的行内节点 `linkMention{url}`（行内提及：图标 + 标题的小块）。Markdown 写成 `<url>`；`[文字](url)` 和正文里裸露的网址仍是普通链接。
- `image{src, alt, title, width, caption}`：`width` 是占正文宽度的百分比（10–100，不写 = 原始大小但不超过正文宽）；`caption` 图注（纯文字）。
  Markdown 照旧是 `![alt](src "title")`，**图注和宽度不进 Markdown**（见下一条）。
- **展示属性**：`embed.view`、`image.width`、`image.caption`、`codeBlock` 的 `wrap` 等只影响显示、Markdown 里没有的属性。Agent 用 Markdown 改写时，
  服务器的 reconcile 对同一块（沿用了编号的）保留旧的展示属性，不因为 Markdown 里没有就清掉。展示属性的清单写在 `oad-markdown` 的 node.rs 里，前后端共用这一份说明。
- 网页和服务器的 schema 照旧一字不差；`packages/editor/test/fixtures` 的对照测试加上新节点、新属性。

## 链接解析（`crates/oad-unfurl`）

把 `spikes/unfurl/` 做成正式 crate（spike 之后删掉）。输出格式就是 spike 的 `Unfurl`（kind / provider / title / author / thumbnail / media / embed ...）。

- `GET /api/v1/unfurl?url=<网址>` → `Unfurl`。要登录（防止被当成开放代理）。
- **缓存**：表 `unfurls(url_key, data, status, fetched_at, expires_at, error)`；`url_key` 是规范化后的网址（去掉 utm_ 之类跟踪参数、统一 youtu.be 等）。
  有效期：帖子、视频 7 天，GitHub 1 小时，普通网页 1 天；失败的按 1 分钟、10 分钟、1 小时退避。过期了先返回旧的（`stale: true`），后台刷新。
  同一网址同时只抓一次。按网站限速（每个域名同时最多 2 个、每秒最多 5 个）。
- **图片转存**：卡片里的图（封面、头像、帖子图片、网站图标）由服务器下载存到自己这里，`Unfurl` 里的图片地址换成 `/api/v1/media/<hash>`，
  解决 B 站、公众号防盗链，原帖删了也能看。视频文件不转存（太大），前端用 `referrerpolicy="no-referrer"` 直接放。
- Agent 或网页写入单独一行网址时，服务器在后台先解析一次（网页打开时多半已经在缓存里）。
- 抓取走 `OAD_FETCH_PROXY`（可选），SSRF 检查同第 2 阶段的 webhook。
- 上传的文件网址（`/api/v1/files/...`）也能解析：直接从库里给 kind（image / video / audio / file）、文件名、大小、类型、时长、封面。

## 文件和图片

- `BlobStore` trait（第 1 阶段计划里有），本地目录实现 `OAD_DATA_DIR/blobs/`，按内容 SHA-256 存（同样的文件只存一份）。S3 以后补。
- `POST /api/v1/workspaces/{ws}/files`（`multipart/form-data`，字段 `file`；或原始正文 + `X-Filename` 头）→
  `{ id, url, name, mime, size, width?, height?, blurhash?, duration_ms? }`。上限 `OAD_MAX_UPLOAD_MB`（默认 100）。
  按内容判断类型，不信客户端给的；SVG 以附件方式下发（`Content-Disposition: attachment`）或带 `Content-Security-Policy: sandbox`。
- `GET /api/v1/files/{id}/{name}`：要能看这个工作区；支持 Range（视频拖进度）；`?w=<宽>` 给图片的 webp 缩略图（宽度取 160 / 320 / 640 / 960 / 1280 / 1920 中最近的，生成后缓存）。
  长缓存（`Cache-Control: private, max-age=31536000, immutable`）。
- `GET /api/v1/media/{hash}`：转存的外部图片，同样支持 `?w=`。
- 图片上传时算 blurhash 和宽高，网页先画模糊占位。
- `upload_file` MCP 工具：base64 内容 + 文件名 → 返回 Markdown（图片是 `![](url)`，别的是单独一行网址）。

## 网页

- **粘贴**：空行里贴网址 → embed 块（立刻出骨架）+ 小菜单（嵌入 / 书签 / 提及 / 链接，回车选默认）；文字中间贴网址 → 链接；选中文字后贴网址 → 给选中的文字加链接；
  贴图片 / 文件 → 上传；贴 Markdown（纯文本里像 Markdown 的）→ 用 WASM 版 oad-markdown 解析成块；贴 HTML → ProseMirror 默认处理再按 schema 清理。
- **卡片**（按 `kind` / `provider`）：YouTube、B 站（封面 + 时长，点了才换成播放器 iframe；YouTube 用 youtube-nocookie，带开始时间）；
  X（自己画：作者、认证标记、正文里的链接 / @ / #、图片网格、视频（封面，点了播放 mp4）、引用的帖子、链接卡片、时间、赞和回复数，点“在 X 上查看”跳原帖）；
  GitHub 仓库 / issue / PR；公众号文章；普通书签（标题、描述、网站图标、大图）；oEmbed 的 rich / video（Spotify、Vimeo、Figma、Loom 用各自的官方 iframe 地址；
  其余 oEmbed HTML 只放进 `sandbox` iframe，不给 `allow-same-origin`）；上传的视频（`<video>`）、音频、PDF（浏览器自带预览，按需）、附件（图标 + 文件名 + 大小 + 下载）。
- 解析结果在 IndexedDB 里也存一份，离线 / 再打开时直接画。
- **图片**：拖左右边改宽度（写 `width`），图注，点开大图（灯箱），懒加载 + `srcset`（`?w=`）+ blurhash 占位。
- **代码块**：Shiki 高亮（按需加载语言），语言选择，复制按钮，自动换行开关；`mermaid` 语言画成图（Mermaid 按需加载），能切回源码。
- **公式**：块级和行内的可视化编辑（输入 LaTeX 实时预览，第 1 阶段已有基础）。
- **块把手**：块左边的 ⋮⋮，拖动排序（含拖进 / 拖出列表、引用、提示框），点开块菜单：转换成…、颜色（以后）、复制块链接（`/p/<page>#<block>`）、复制、删除；
  打开带 `#<block>` 的链接时滚到那块并闪一下。
- **性能**：嵌入块都是“门面”（封面图 + 按钮），点了才加载 iframe / 视频；离开视口的 iframe 不卸载但暂停（能做到的话）；图片 `loading=lazy`、`decoding=async`；
  主包不因为这些变大（卡片渲染、Shiki、Mermaid、WASM 都按需加载）。

## WASM 版 oad-markdown

- `crates/oad-markdown` 加 `wasm` feature，用 wasm-bindgen 导出 `parse(md) -> JSON` 和 `serialize(JSON) -> md`；打包成 `packages/markdown-wasm`（@oad/markdown），网页按需加载。
- 测试：同一组 Markdown 在 Rust 和 WASM 里结果一样。

## 实际格式（服务器实现，网页按这个对）

**文档**

- `linkMention{url}` 是行内原子节点（和 hardBreak、mathInline 一样，没有块编号）。Markdown：`<https://…>` ↔ linkMention；加了格式的（`**<https://…>**`）、
  `[文字](网址)`、正文里裸露的网址都是链接 mark。
- 展示属性的清单是 `oad_markdown::node::PRESENTATION_ATTRS`（embed.view、image.width、image.caption、codeBlock.wrap）。reconcile 时新节点**没写**的展示属性保留旧值，
  写成 `null` 的才清掉（网页发整页 JSON 时要清就写 null）。

**链接解析**

- `GET /unfurl?url=` → 200 Unfurl（不含 `attempts`）。解析失败（包括被 SSRF 检查拦下的地址）→ 502 `{ error: { code: "bad_gateway" } }`；失败也缓存，退避期内直接回 502。
  过期了给旧的并带 `stale: true`，后台刷新；刷新失败时还用旧的，退避后再试。
- 转存后的图片地址是 `/api/v1/media/<sha256>`（要登录，任何工作区的人都能看；`?w=` 同文件）。`thumbnail` 转存成功时补上 `width`、`height`、`blurhash`。
  解析时最多等图片转存 1.5 秒：没转完的先给原地址（网页用 `referrerpolicy="no-referrer"` 画），后台转完更新缓存，之后再要就是本地地址。
  转存的是 `favicon`、`author.avatar`、`thumbnail.url`、`media[]` 里图片的 `url`、视频的 `poster`、X 帖子（含引用）里的这些、公众号头像、X 链接卡片的 `image`；视频、GIF 文件本身不转存。
- 上传的文件网址（`/api/v1/files/<id>/…`，相对或绝对）：`{ kind: "image" | "file", provider: "upload", source: "api", url, title, name, mime, size, width?, height?, blurhash?, duration_ms? }`，
  图片另有 `thumbnail{url, width, height, blurhash}`。不是那个工作区的成员 → 404。
- 缓存键：去掉跟踪参数（utm_*、fbclid、gclid、si、spm_id_from、vd_source、X 的 s / t…），youtu.be、m.youtube、shorts 统一成 `www.youtube.com/watch?v=`，
  twitter.com 统一成 x.com，域名小写、去掉 #、查询参数排序（`oad_unfurl::cache_key`）。
- 预取：顶层 embed 块（Agent 经 HTTP / MCP 写的、浏览器同步过来的都算）的网址交给后台，攒 300ms 去重后解析；同一页一次加载里同一个网址只交一次。
- 抓取配置：`OAD_FETCH_PROXY`（代理），`OAD_FETCH_ALLOW_CIDRS`（允许的内网网段，例如开 Clash fake-ip 的本机开发机 `198.18.0.0/15`）。

**文件**

- 内容存 `OAD_DATA_DIR/blobs/`（按 SHA-256，同样的只存一份）；缩略图也存在那里（`<hash>.w<宽>.webp`）。
- `POST /workspaces/{ws}/files` → 201 `{ id, url, name, mime, size, width?, height?, blurhash?, duration_ms? }`；要写权限。原始正文时文件名放 `X-Filename`（可以百分号编码）。
  超过 `OAD_MAX_UPLOAD_MB` → 413 `too_large`。文件名去掉路径和控制字符。
- 类型按内容判断：认不出的文本是 `text/plain`（.md → text/markdown、.csv → text/csv、.json → application/json），HTML 一律当 `text/plain`，二进制 `application/octet-stream`。
- `GET /files/{id}/{name}`（`{name}` 只是给人看的，不参与查找）：`Cache-Control: private, max-age=31536000, immutable`，`ETag`（`If-None-Match` → 304），
  `Accept-Ranges: bytes`、单段 `Range` → 206（不合法 → 416），`X-Content-Type-Options: nosniff`；SVG、文本、XML 带 `Content-Security-Policy: sandbox`，SVG 是 `attachment`。
  不是工作区成员 → 404。
- `?w=`：JPEG / PNG / WebP / BMP 给有损 webp（质量 80），宽度取 160 / 320 / 640 / 960 / 1280 / 1920 里最近的一档；原图不比这宽、SVG、GIF 直接给原图。
- 宽高、blurhash：位图（含 GIF 第一帧）都算；`duration_ms`（和视频宽高）目前只从 MP4 / MOV / M4A 里读。

**MCP**：`upload_file(name, content_base64)`（也接受 `data:…;base64,` 前缀）→ 文字里最后一行是能直接放进页面的 Markdown（图片 `![](url)`，别的是单独一行网址）。

**WASM（packages/markdown-wasm，@oad/markdown）**：`wasm-bindgen --target web`，默认导出 init，另有 `parse(md) → JSON 字符串`、`serialize(JSON 字符串) → md`、
`serializeBlock(JSON 字符串) → md`。`sh packages/markdown-wasm/build.sh` 重新编（要 wasm32-unknown-unknown 和 wasm-bindgen-cli 0.2.130）；编好的 dist 跟着提交，
网页的 Vite alias 指向它。

**网页静态文件**：打进二进制的 dist 预先压好 brotli、gzip（图片、字体放原文），按 `Accept-Encoding` 给，`Vary: Accept-Encoding`、`ETag`；`assets/` 下长缓存，别的 `no-cache`。
