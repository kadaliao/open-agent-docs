# 第 5 阶段约定（搜索、历史、回收站、分享、双语、快捷键、移动端）

在前几个阶段上加。服务器和网页并行做；改约定先改这份文件。目标：功能不少于旧版（`legacy/`），搜索达标。

## 验收

1. 搜索 10 万段：p95 < 20ms；中文按词（jieba）、英文按词干；标题权重高；能搜到数据库行的属性；结果带命中的片段和高亮；
   网页 ⌘K 打开搜索面板，边打边出结果（同时能跳到最近看过的页面、执行命令）。
2. 历史：每页自动存版本（编辑停下 2 分钟、或累计改动多时存一版，Agent 的每次写入单独成版），能看每一版、和现在对比（按块显示增删改、谁改的），恢复到某一版
   （恢复是一次新的改动，不丢后来的历史）。数据库的格子和表结构改动也能看历史、撤销。
3. 回收站：删掉的页面、库、行能找回、彻底删除；30 天后自动清。
4. 分享：页面可以生成只读的公开链接（可选带子页面、可选允许搜索引擎、可随时关），公开页面不需要登录，渲染快（服务器直接出 HTML + 轻量脚本，不加载编辑器）。
5. 中英双语：界面所有文字、服务器返回给人看的错误、MCP 工具描述和返回（按 Agent 的语言设置），切换不刷新。
6. 快捷键：和 Notion 习惯一致的一套（⌘K、⌘P、⌘/、⌘⇧L 换主题、块操作、Markdown 快捷输入），有快捷键帮助面板。
7. 移动端：手机浏览器能看、能改（侧栏变抽屉、工具栏适配触屏、表格横向滚动、看板竖排），PWA（可加到主屏幕、离线能打开看过的页面）。

## 搜索（服务器，`crates/oad-search`）

- tantivy + jieba（`tantivy-jieba` 或自己接分词器）；索引放 `OAD_DATA_DIR/search/`。
- 文档：页面（标题、正文按块切成段落，每段一条，带 page_id、block_id）、数据库行（标题 + 文本类属性）、评论（以后）。
- 增量：页面的同步中心在存盘时把变了的块送去重建索引（防抖 1 秒）；删页 / 恢复同步。启动时只补漏掉的（按 seq 对比），不全量重建。
- 权限：结果按工作区过滤。
- `GET /workspaces/{ws}/search?q=&limit=&kind=page|row|all` → `{ results: [{ page_id, block_id?, title, snippet（带 <mark>）, kind, score, updated_at }] }`
- MCP 的 `search_pages` 换成这个。

## 历史

- `doc_versions(page_id, id, snapshot（Yrs 快照）, state_vector, created_at, authors（这一版里改过的人）, kind: auto | agent | restore | manual, label?)`。
  文档本来就不回收删掉的内容（`skip_gc`），快照很小。
- `GET /pages/{id}/versions` → 列表；`GET /pages/{id}/versions/{v}` → `{ doc, markdown }`；`GET /pages/{id}/versions/{v}/diff?to=<v2|current>` → 按块的增删改；
  `POST /pages/{id}/versions/{v}/restore`；`POST /pages/{id}/versions`（手动存一版，带名字）。
- 数据库：`GET /databases/{id}/history?row=&prop=` 用已有的 `db_ops` / `db_schema_ops`；`POST /databases/{id}/history/undo` `{ seq }`。
- MCP：`page_history(page_id)`、`restore_version(page_id, version)`。

## 回收站

- `GET /workspaces/{ws}/trash`、`POST /trash/{page_id}/restore`（连子页面；父页面不在了就放到根上）、`DELETE /trash/{page_id}`（彻底删：文档、评论、文件引用、索引）。
- 后台任务每天清 30 天前的。

## 分享

- `shares(id, page_id, token, include_children, indexable, created_by, created_at, revoked_at)`；`POST /pages/{id}/share` → `{ url }`，`DELETE /pages/{id}/share`。
- `GET /s/{token}`（和子页面 `/s/{token}/{page_id}`）：服务器用 oad-markdown 的节点树直接渲染成 HTML（卡片用解析缓存画成静态卡片、图片用缩略图），
  一个小脚本负责代码高亮、公式、视频门面；`noindex` 除非 `indexable`。不需要登录；只读。
- 数据库分享：只读的表格视图（服务器渲染前 200 行，滚动再取）。

## 双语

- 网页：所有文字进 `strings`（已经集中了），加英文；跟浏览器语言，设置里可改，存在用户偏好里（`PATCH /me` `{ locale }`）。
- 服务器：错误信息、通知 prompt、MCP 工具描述和返回按 `Accept-Language` / 用户或 Agent 的 `locale`；默认中文。

## 网页其他

- ⌘K 搜索面板（搜索 + 最近页面 + 命令）、快捷键帮助（⌘/）、页面历史侧栏（版本列表、对比、恢复）、回收站页面、分享弹窗、
  设置页（个人：名字、头像、语言、主题；工作区：名字、成员和邀请链接）、移动端适配、PWA（service worker 缓存静态资源和看过的页面）。

## 成员和邀请（服务器）

- `POST /workspaces/{ws}/invites` `{ role, expires_in? }` → `{ url }`（`/invite/<token>`，打开后登录或注册即加入）；`GET` / `DELETE` 管理邀请。
- `PATCH /workspaces/{ws}/members/{user}` `{ role }`、`DELETE` 移出；`PATCH /workspaces/{ws}` `{ name, icon }`；`PATCH /me` `{ name, avatar, locale }`、改密码。
- local 模式下的注册：`OAD_SIGNUP=invite`（默认，只能凭邀请注册）| `open` | `off`。

## 网页按这些接（约定里没写细的地方；服务器做得不一样就改这里，网页跟着改）

- 搜索结果的 `snippet`：服务器把原文转义过，只留 `<mark>` / `</mark>`；网页照样插进去（不再转义）。`kind` 可以多一种 `database`；可选 `icon`。
- 版本：`GET /pages/{id}/versions` → `{ versions: [{ id, created_at, kind, label?, authors: [{ kind, id, name }] }] }`（新的在前）。
  `GET .../versions/{v}/diff?to=current` → `{ blocks: [{ op: same | added | removed | changed, id?, type, before?（那一版的 Markdown）, after?（现在的）, by?: [人] }] }`，按现在的顺序排，删掉的插在原来的位置。
  `POST /pages/{id}/versions { label }` → 新版本；`POST .../restore` → 200。
- 数据库历史：`GET /databases/{id}/history?row=&prop=` → `{ items: [{ seq, kind: cell | schema | row, row_id?, row_title?, prop?, prop_name?, old?, new?, summary?（表结构改动的说明）, by: { kind, id, name } 或 "user:<id>", at, undone? }] }`（新的在前，最多 200 条）。
  `POST /databases/{id}/history/undo { seq }`（格子和表结构都行）。
- 回收站：`GET /workspaces/{ws}/trash` → `{ items: [{ id, title, icon, kind, deleted_at, deleted_by, parent_title? }] }`；`POST /trash/{id}/restore` → 恢复的页面；`DELETE /trash/{id}` → 204。
  恢复时发 `page.created`（侧栏靠它出现）。
- 分享：`GET /pages/{id}/share` → `{ url, token, include_children, indexable, created_at }` 或 404（没分享）；`POST` 带 `{ include_children, indexable }`，已经分享时是改设置（链接不变）。
- 邀请：`GET /workspaces/{ws}/invites` → `{ invites: [{ token, url, role, expires_at?, created_at, uses }] }`；`expires_in` 是秒，null 是一直有效；
  `GET /invites/{token}`（不用登录）→ `{ workspace: { id, name, icon }, role, invited_by?: { name }, expires_at?, signup: 能不能凭它注册 }`，无效 404 / 410；
  `POST /invites/{token}/accept`（已登录）→ 新的 `/me`；`POST /auth/signup { email, password, name, invite? }` → `/me`（同时登录、加入）。
- 个人：`PATCH /me { name?, avatar?（上传后的文件网址，null 去掉）, locale?: zh | en | null }` → `/me`；`/me` 的 `user` 带 `avatar`、`locale`；`POST /me/password { current, new }`。
  工作区：`PATCH /workspaces/{ws} { name?, icon? }`；`/me` 的 `workspaces[]` 带 `icon`；角色多一个 `admin`（成员、邀请、工作区设置）。
- 公开页面 `/s/{token}`：网页不管，服务器出；排版照编辑器（字体栈、字号 16px、行高 1.65、正文宽 720px、标题 40px），深浅色跟系统。

## 服务器（实际的样子）

- **时区**：服务器用系统的 IANA 时区（按当时的偏移算，夏令时对），`OAD_TZ`（如 `Asia/Shanghai`）可以改；`GET /auth/config` → `{ mode, timezone, signup }`。
  `PATCH /me` 也收 `timezone`（IANA 名字，先存着，数据库还是按服务器的算）。
- **搜索**（`crates/oad-search`，索引在 `OAD_DATA_DIR/search/`）：`GET /workspaces/{ws}/search?q=&limit=（默认 20，最多 100）&kind=page|row|all&per=block`
  → `{ results: [{ page_id, block_id?, title, snippet, kind: page | database | row, icon?, score, blocks, updated_at, db_id? }] }`。
  默认每页一条（分最高的那一处的片段，`blocks` 是这页命中了几处）；`per=block` 每块一条（同一页最多两条）。
  最后一个词按前缀算（边打边搜）。索引防抖 1 秒；启动时按每页的 `updated_at`、每个库的 `seq` 补漏掉的，标记存在索引目录的 `marks.json`。
  1 万行的库、10 万段：p95 约 3ms（`cargo test --release -p oad-search --test bench -- --ignored --nocapture`）。
- **历史**：自动存版本——编辑停下 2 分钟、一直在改 10 分钟或攒了 64KB 增量、Agent 的每次写入（之前别人没存的先存一版）、页面卸载 / 关机前。
  版本 `{ id, page_id, kind: auto | agent | restore | manual, label?, authors: [{ kind, id, name }], created_by, created_at }`。
  `GET /pages/{id}/versions/{v}` → `{ version, doc, markdown }`；diff 的块 `{ op, id, type, before?, after?, by? }`；恢复 → `{ version }`（新的一版，`label` 按恢复的人的语言：「恢复到第 N 版」/「Restored version N」）；
  `POST /pages/{id}/versions { label? }` → 201 新版本。
- **数据库历史**：`GET /databases/{id}/history?row=&prop=（key 或名字）&limit=` → `{ items, seq }`，`items` 同上面网页约定（`kind: cell | row | schema`，`by` 是 `{ kind, id, name }`，`summary` 按语言）。
  `POST .../history/undo { seq }`：表结构的走表结构撤销；格子的换回原值（之后又被改过的不动）、加的行删掉、删的行放回。
- **回收站**：`items[]` 多 `parent_id`；`POST /trash/{id}/restore` → 放回来的页面，加 `restored: [id]`（连同一起删的）；彻底删连正文、版本、评论、行、分享、索引，
  没有别的页面用过的上传文件（`file_refs`，只加不减）和它的缩略图也删；发 `page.purged`。每天清 30 天以上的（起来 1 分钟后先清一次）。
- **分享**：`/s/{token}`、`/s/{token}/{page}`（带子页面时）；图片 `/s/{token}/_/files/{id}/{name}?w=`（要是分享范围里的页面用过的）、转存图 `/s/{token}/_/media/{hash}`、
  表格往下翻 `/s/{token}/_/rows/{db}?offset=` → `{ html, next? }`。行页面跟着库走。CSP 只放行本页 nonce 的脚本和 jsDelivr（highlight.js、KaTeX、Mermaid 用到时才加载）、
  YouTube nocookie / B 站播放器的 iframe。按浏览器语言出中文或英文。首字节：100 块 0.9ms、1000 块 10ms、1 万块 65ms（`--test share share_ttfb`）。
- **成员**：`POST /workspaces/{ws}/invites` → 201；`DELETE /workspaces/{ws}/invites/{token}` → 204；`PATCH /workspaces/{ws}/members/{user} { role }` → `{ user_id, role }`；
  `DELETE` → 204（自己退出也行）。规矩：admin 以上能管；只有 owner 能动 owner、邀请 / 设成 owner；不能给比自己高的角色；至少留一个 owner；Agent 不能管成员（Agent 在 Agent 设置里撤销）。
  `PATCH /workspaces/{ws}` 发 `workspace.updated`。`POST /auth/signup` → 201 `/me`（带会话 cookie）；`POST /me/password` → 204（别处的登录都退出，这次的留着）。
  Agent 的语言：`PATCH /agents/{id} { locale }`。
- **双语**：接口错误的 `message` 按 用户 / Agent 设的语言 > `Accept-Language` > 中文 翻（英文时带 `Content-Language: en`）；Agent 的通知 prompt 按 Agent 的语言；
  MCP 的工具描述、参数说明、返回按 Agent 的语言。字符串表每个 crate 一处：`oad_core::i18n::CORE`（和翻 oad-db 引擎报错的 `DB_ENGINE`）、`oad_server::i18n::SERVER`、`oad-mcp` 的 `MCP`。
