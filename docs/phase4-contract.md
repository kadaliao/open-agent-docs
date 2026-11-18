# 第 4 阶段约定（数据库）

在前几个阶段上加。服务器和网页并行做，按这里对接；改约定先改这份文件。功能参考旧版 `legacy/open_agent_docs/dbs.py`、`dbcalc.py`（不兼容它的格式）。

## 验收

1. 1 万行的库：表格滚动 60fps；筛选、排序、分组 < 50ms（在浏览器里算）；有缓存打开 < 100ms，没缓存 < 500ms。
2. 表格 / 看板 / 日历 / 画廊 / 列表五种视图，每个视图有自己的筛选（可嵌套的且 / 或）、排序、分组、隐藏的列、列宽。
3. 公式（Notion 风格）、关联（含双向）、汇总；浏览器和服务器算出来一样（同一份 Rust 代码，浏览器里是 WASM）。
4. 两个人同时改不同格子都留住，同一格后写的为准；别人改的格子实时出现。
5. 页面里能嵌入数据库视图（单独一行库的网址）。
6. Agent 能经 MCP 建库、改表结构、加行、改行、按条件查（写名字，不用写编号）；改表结构能撤销。
7. 导入 / 导出 CSV。

## 模型

- 数据库是一种页面（`pages.kind = "database"`），行也是页面（`kind = "row"`，`parent_id` = 库）。行有自己的正文（CRDT 文档，和普通页面一样能编辑、评论、Agent 按段改）。
  侧栏不列出行。
- **表结构** `schema`：`{ props: [{ key, name, type, options?, format?, formula?, relation?, rollup?, ... }], next_id }`。`key` 是短随机串，改名不动数据。
  第一个属性是 `title`（行的标题，就是行页面的标题），不能删。
- **类型**（同旧版）：title、text、number、select、multi_select、status、date、person、checkbox、url、email、phone、files、
  created_time、updated_time、created_by、updated_by、relation、rollup、formula、unique_id、button。
  选项 `{ id, name, color, group? }`（status 有 group：todo / doing / done）；值存选项 id。
- **值**：存的样子（JSON）——文字是字符串（行内格式以后再说，先纯文字）、数字、布尔、`{ start, end?, time? }`（日期，ISO）、
  选项 id / 选项 id 数组、`["user:<id>", "agent:<id>"]`、`[{ url, name }]`（文件）、行 id 数组（关联）。空值不存。
- **视图** `views: [{ id, name, type: table | board | calendar | gallery | list, filter, sorts, group_by?, sub_group_by?, hidden: [key], order: [key],
  widths: {key: px}, calendar_by?, cover?（画廊封面：files 属性或 "body"）, card_size?, wrap? }]`。
  `filter`：`{ op: "and" | "or", items: [ { prop, cond, value } | filter ] }`，`cond` 按类型：eq、neq、contains、not_contains、starts_with、
  gt、gte、lt、lte、empty、not_empty、is_within（日期：past_week、next_month…）、checked…。
- **存储**：`db_rows(row_id, db_id, seq, props JSON, created_at, created_by, updated_at, updated_by, unique_id)`；每格最后写入为准。
  `db_ops(db_id, seq, row_id, prop, old, new, by, at)` 操作日志（历史、撤销、增量同步都用它）；`seq` 是库内递增的版本号。
  表结构的改动记 `db_schema_ops(db_id, seq, op, before, after, by, at)`，能撤销（删掉的列和值放得回来）。

## 计算引擎：`crates/oad-db`（纯逻辑，不碰 IO）

服务器直接用；`wasm` feature 编成 `packages/db-wasm`（@oad/db），网页按需加载。两边用同一套代码，测试里对照。

- 值的规范化：网页写存的样子；Agent 写名字（“进行中”、“张三”、“2026-11-15”、“是”），都认，读给 Agent 的一律是名字。
- 公式：语法参考 Notion（`prop("名称")`、`+ - * / % ^`、比较、`and or not`、`if`、`条件 ? 甲 : 乙`、`let`，函数同旧版 FUNCS，加 Notion 2 的方法写法 `prop("标签").length()`）。
  改表结构时解析，写错了报清楚哪里错；读的时候按行算，不存；循环引用、太深报错。
- 关联：存对方的行 id 列表；双向的在对方库有反向属性（`mirror_of`），值不存，读的时候反查。汇总：沿关联取对方一个属性，按函数归总（count、sum、avg、min、max、earliest、latest、percent_checked、show_original…）。
- 查询：筛选、排序、分组、汇总行（每列底部的计数 / 求和…）；1 万行 < 20ms（WASM）。
- WASM 接口（网页用）：

```ts
const eng = await DbEngine.create(schema)             // schema JSON
eng.loadRows(rows)                                   // [{ id, seq, props, created_at, ... }]
eng.loadRelated(dbId, schema, rows)                  // 关联、汇总要用到的别的库
eng.apply(changes)                                   // 增量：[{ row_id, props? , deleted? }]，返回受影响的行 id
eng.setSchema(schema)
eng.query(view) -> { ids: string[], groups?: [{ key, name, color, ids, count }], totals?: {key: value} }
eng.computed(rowId) -> { [key]: value }               // 公式、汇总、反向关联
eng.display(key, value) -> string                     // 给人看的文字（日期、数字格式、选项名）
eng.checkFormula(src) -> { ok } | { error, at }
eng.normalize(key, input) -> value                    // 用户输入 / 粘贴的文字 → 存的样子
```

## HTTP

- `POST /workspaces/{ws}/databases` `{ title, parent_id?, after?, schema?, views? }` → 库（也是一个 page，带 schema、views）
- `GET  /databases/{id}` → `{ page, schema, views, seq }`
- `GET  /databases/{id}/rows?since=<seq>` → `{ seq, rows: [...], deleted: [row_id] }`（不给 since 是全量；全量支持 gzip / br，1 万行 < 1MB 压缩后）
- `POST /databases/{id}/rows` `{ rows: [{ props, after? }] }` → `{ ids, seq }`（发 unique_id）
- `PATCH /databases/{id}/rows` `{ ops: [{ row_id, set: { key: value } }] }` → `{ seq }`；值用存的样子，也认名字
- `DELETE /databases/{id}/rows` `{ ids }`（进回收站）
- `PATCH /databases/{id}/schema` `{ ops: [{ op: add | rename | change_type | delete | set_options | set_formula | ..., ... }] }` → `{ schema, seq }`；改类型时能转的值转过去
- `POST /databases/{id}/schema/undo` `{ seq }` 撤销某次表结构改动
- `PUT  /databases/{id}/views` / `POST` / `PATCH /databases/{id}/views/{view}` / `DELETE`
- `POST /databases/{id}/query` `{ view_id? , filter?, sorts?, limit?, offset? }` → 服务器算（给 Agent、给不想加载全部数据的客户端）
- `POST /databases/{id}/button/{row}/{key}` 按按钮
- `GET  /databases/{id}/export.csv`、`POST /workspaces/{ws}/databases/import`（multipart CSV → 新库，猜类型）
- 实时：工作区 SSE 加 `db.changed { db_id, seq }`（网页收到后 `rows?since=`）和 `db.schema { db_id }`。

## MCP

| 工具 | 作用 |
| --- | --- |
| `db_create(title, props?, parent_id?)` | 建库，`props` 是 `[{ name, type, options? , formula? ... }]` |
| `db_schema(db_id, ops)` | 改表结构（同 HTTP，名字代替 key） |
| `db_query(db_id, filter?, sorts?, view?, limit?)` | 查，返回 Markdown 表格（名字，不是 id），带行 id 和总数 |
| `db_add(db_id, rows)` | 加行：`[{ "名称": "…", "状态": "进行中", ... , body?: "Markdown" }]` |
| `db_update(rows)` | 改行：`[{ row_id, set: { "状态": "已完成" } }]` |

行的正文照旧用 `read_page` / `edit_page`（行 id 就是页面 id）。`read_page` 读行时先列属性。

## 网页

- 库页面：标题、视图切换标签、工具栏（筛选、排序、分组、属性显隐、搜索、新建）；视图设置都存进视图（大家共享），本人临时的搜索不存。
- **表格**：TanStack Virtual 虚拟滚动（行和列）；列头菜单（改名、改类型、排序、筛选、隐藏、删除、插入左右列）；拖列宽、拖列顺序；
  格子编辑器按类型（文字、数字、选项下拉带新建选项、日期选择器、人员选择、勾选、文件上传、关联选择行、网址）；键盘导航（方向键、回车编辑、Esc、Tab、复制粘贴多格）；
  每列底部汇总；分组折叠；行左边打开整页。
- **看板**：按 select / status / person 分列，卡片拖动改值、列内排序，卡片显示选中的属性；**日历**：按日期属性，月 / 周，拖动改日期；
  **画廊**：卡片带封面（文件属性或正文第一张图）；**列表**：紧凑的行列表。
- **行页面**：上面是属性（可编辑），下面是正文编辑器（和普通页面一样协作）；从表格打开是右侧滑出的面板，可以展开成整页。
- 公式编辑器：带属性、函数补全和实时报错。
- 嵌入：页面里单独一行库的网址（`/p/<库 id>`，可带 `?view=<视图>`）显示成可交互的内嵌视图（embed 的解析结果 `kind = "database"`）。
- 数据先从 IndexedDB 出，再 `rows?since=` 增量；本地改动先显示（乐观），失败回滚。
