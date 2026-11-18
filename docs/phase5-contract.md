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
