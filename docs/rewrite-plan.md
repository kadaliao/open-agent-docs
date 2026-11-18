# 重写计划（next 分支）

把 Open Agent Docs 从 Python + FastAPI 重写成 Rust 服务 + 新前端，同时做成别人能嵌进自己服务的 SDK。
旧版整个放在 `legacy/`，正式切换前一直能跑（`cd legacy && uv run pytest`）。

## 已经定了的

- **不兼容旧版**。HTTP 接口、MCP 工具、通知格式、数据格式都按最好的设计重做；接入方（DSH Bots 等）还没用上，跟着改。
- **首版只支持 SQLite**，存储做成接口（`Storage` trait），Postgres 第 6 阶段再补。
- **在这个仓库的 `next` 分支做**，Python 版挪进 `legacy/`。

## 目标（每个阶段按这些数验收）

| 指标 | 目标 |
| --- | --- |
| 冷启动 / 空闲内存 / 二进制大小 | < 50ms / < 30MB / < 30MB（前端打进同一个文件） |
| 打开 1000 段的页面 | 有本地缓存 < 50ms 出内容，没缓存 < 200ms |
| 打字延迟 | 每次按键 < 16ms，有人或 Agent 同时在写也一样 |
| 数据库视图 1 万行 | 滚动 60fps，筛选、排序 < 50ms |
| 搜索（10 万段） | p95 < 20ms |
| 贴 X / YouTube 链接 | 立刻出卡片骨架，有缓存 < 300ms 出完整卡片 |

旧版的基线数据记在 `docs/baseline.md`。

## 技术选型

**后端 Rust**：axum + tokio、rusqlite（WAL）、rmcp（MCP）、tantivy + jieba（搜索）、`image`（webp 缩略图、blurhash）、
rust-embed（前端打进二进制）、reqwest（链接解析、webhook）。

**文档用 CRDT**。人和多个 Agent 同时写不冲突，网页上实时看到 Agent 在写哪一段，离线编辑、按人撤销、版本回溯都靠它。
**用 Yjs（浏览器，Tiptap Collaboration / y-prosemirror）+ Yrs（服务器）**，比较见 `spikes/crdt/README.md`。块编号是文档节点上稳定的 `id` 属性；
Agent 改某一段 = 服务器把它给的 Markdown 转成节点，作为一次 CRDT 事务写进去，连着的网页马上看到。

**数据库的行属性存 SQL 表**（要查询、建索引），每个格子按最后写入为准，另记操作日志供历史回看；行的正文是 CRDT 文档。

**前端**：React + Tiptap（ProseMirror）。本地优先（IndexedDB 存快照，打开先显示缓存再同步）；TanStack Virtual 做数据库表格；
公式引擎、Markdown 互转用 Rust 写，同时编译成 WASM 在浏览器里跑，前后端结果一致；Mermaid、KaTeX、Shiki 按需加载。

## 富媒体

服务器端的链接解析（`oad-unfurl`），按网站分别处理，不认识的退到通用：

- **X**：syndication 接口拿正文、作者、图片视频，前端自己画卡片，内容存一份到本地（原推删了也能看）。
  这个接口不是公开的，失效时退到 oEmbed，再退到 Open Graph。
- **YouTube**：只放封面，点了才加载 youtube-nocookie 播放器；支持从指定时间开始。
- **其他**：B 站、GitHub（仓库 / issue / PR）、Figma、Loom、Spotify、公众号文章……；剩下的用 Open Graph / Twitter Card + 网站图标做书签卡片。
- **交互**：空行粘贴链接弹出“嵌入 / 书签 / 行内提及 / 普通链接”，之后随时能换。
- **Agent 那边**：单独一行的网址就是卡片，显示成哪种是这一段的属性，不写进 Markdown。
- **安全**：DNS 解析后拒绝内网和回环地址，限重定向次数、大小、超时；图片转存到本地；抓取代理 `OAD_FETCH_PROXY`。
- **其他媒体块**：视频、音频、PDF 预览、附件、图集和图注、公式、Mermaid、提示框、分栏。

## 代码结构（也就是 SDK 的形态）

```
crates/
  oad-core       领域逻辑：页面树、CRDT 文档、评论、Agent、通知；不含 HTTP
                 可替换：Storage / BlobStore / AuthProvider / TenantResolver
  oad-markdown   Markdown ↔ 文档（也编成 WASM）
  oad-formula    公式引擎（也编成 WASM）
  oad-unfurl     链接解析
  oad-server     axum Router，别的 Rust 服务直接挂进去
  oad-mcp        MCP 端点
  oad-bin        单个二进制 open-agent-docs
  oad-node / oad-py   napi-rs / PyO3 绑定（第 6 阶段）
packages/
  editor         @oad/editor：能嵌进别人页面的 React 编辑器
  client         @oad/client：TS 客户端（请求、同步）
apps/web         自带的完整网页
spikes/          第 0 阶段的对比实验，定完就删
legacy/          Python 版
```

别人的接法：Rust 服务直接挂 Router；别的语言把二进制当旁挂服务，用户身份由宿主用 JWT 或可信请求头告诉它；
绑定做好后 Node / Python 能在进程内直接用。

## 阶段

| 阶段 | 内容 | 完成标准 |
| --- | --- | --- |
| **0 对比实验**（1 周） | Loro / Yrs 各接一次 Tiptap，Rust 端按块改文档、和浏览器并发编辑；Rust 版 Markdown 互转能原样来回；X / YouTube 解析原型；旧版基线 | 选定 CRDT，写进本文 |
| **1 骨架**（3 周） | 工程结构、存储、页面树、CRDT 文档、WebSocket 同步、在线状态；新编辑器接协作；可替换的登录；单个二进制 | 两个浏览器同时编辑不冲突；打开、打字达标 |
| **2 Agent**（2 周） | 重新设计的 MCP 工具；Agent 按段编辑走 CRDT 事务；评论、通知（webhook / SSE / 轮询）、Agent 管理 | 集成测试全过；Claude Code 接上能用 |
| **3 富媒体**（2–3 周） | 链接解析、粘贴菜单、各种嵌入块、图片处理、视频 / 音频 / PDF、公式、Mermaid、代码块 | X / YouTube 等出完整卡片；10 个视频的页面滚动不掉帧 |
| **4 数据库**（3–4 周） | 表结构、属性、Rust + WASM 公式、关联和汇总；表格 / 看板 / 日历 / 画廊；虚拟滚动 | 1 万行达标 |
| **5 补齐**（2 周） | tantivy 搜索、历史和恢复、回收站、分享页、中英双语、快捷键、移动端 | 搜索达标；功能不少于旧版 |
| **6 SDK 与切换**（2–3 周） | 接口定稿；发 `@oad/editor`、`@oad/client`；napi / PyO3 绑定；Postgres；文档；next 合回 main | 删掉 `legacy/` |

旧版只修 bug，不再加功能。

## 风险

- Agent 按段编辑和 CRDT 的衔接：Agent 改到一半人也在改同一段时怎么处理，第 0 阶段先走通。
- X 的 syndication 接口不是公开的：必须有退路、缓存到本地。
- 公式引擎迁移（旧版 `dbcalc.py` 528 行）：逐个函数对照旧测试移植。

## 第 0 阶段结论

- **CRDT：Yjs + Yrs**（`spikes/crdt/`）。两边正确性都过了（人和 Agent 同时改同一段，字都留得住；JS 和 Rust 合并结果一致），
  决定性的是打字延迟：1000 段时 Yjs 每键 p95 3ms、Loro 15ms；10000 段时 Yjs 8.7ms、Loro 70ms。打开、服务器读、快照大小也是 Yjs 好。
- **Markdown 互转：`crates/oad-markdown`**（正式 crate，不是实验）。comrak 解析，自己写输出；单独一行的网址 = 链接卡片，
  `> [!NOTE]` = 提示框，`<details>` = 折叠块，`$$` = 公式，中文挨着 `**` 也认。每段输出后解析回来核对，对不上就改用 HTML 标签，
  18 万份随机文档来回一致；10000 段输出 80ms、解析 36ms。已知限制：表格的行内代码里写不出“反斜杠紧跟竖线”。
- **链接解析：`spikes/unfurl/`**。YouTube、X（syndication，退路 oEmbed、页面信息）、GitHub、B 站、公众号、通用 Open Graph 都拿到了完整卡片；
  36kr 是 JS 验证页，拿不到。防 SSRF 测过（内网、各种 127.0.0.1 写法、解析到内网的域名、重定向）。
  要点：B 站和公众号的图片防盗链，得转存或 `referrerpolicy="no-referrer"`；要缓存、退避；代理模式下 DNS 由代理做，检查只能尽量。
- **旧版基线：`docs/baseline.md`**。几乎每项都远没达标：冷启动 637ms、空闲内存 79MB、浏览器打开 1000 段页面 748ms、
  1 万行数据库筛选 354ms、搜索 p95 55ms、Agent 改一段 53ms；只有打字 p50 8.6ms（p95 16.9ms）勉强过线。
  主要原因：同步 SQLite 跑在事件循环上、改一段要重切整页（还有两处 O(n²)）、搜索每次先比一遍索引、数据库视图在 Python 里读全表再筛。
- **第 1 阶段已开工：`crates/oad-doc`**（页面正文 = Yrs 文档）。Node ↔ Yrs、按块编号改（Agent 用 Markdown 改写列表，没变的项沿用编号）、
  整页重写、插入删除、增量编解码；人和 Agent 同时改同一段两边的字都留得住。1 万段：建 93ms、加载 40ms、读 39ms、按段改 6ms（按编号找块是线性的，以后加索引）。
