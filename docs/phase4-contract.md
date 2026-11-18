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

## 引擎实现说明

`crates/oad-db` 和 `packages/db-wasm`（@oad/db）按上面的约定做，约定没说清的这样定（改这里先和两边对一下）：

**字段**
- 关联 `relation: { db_id, mirror_of? }`（也认 `db`）；指向自己写自己的 id 或 `"self"`。汇总 `rollup: { relation, prop, fn }`（`prop` 也认 `target`）：`relation` 是这个库的关联属性 key（存的或反向的都行）。
- 数字属性可以有 `precision`（固定几位小数）；日期属性可以有 `date_format`：`iso`（默认，2026-11-15 14:00）| `zh`（2026年11月15日）| `slash` | `relative`（今天 / 明天…）。
- 视图里查询用到的：`filter`、`sorts: [{ prop, dir: "asc" | "desc" }]`（也认 `desc: true`、旧的 `sort`）、`group_by`、`sub_group_by`、`group_date` / `sub_group_date`（日期怎么分组：day | week | month（默认）| year | relative）、
  `hide_empty_groups`、`totals: { key: fn }`、`search`（临时的）、`limit` / `offset`。别的字段（hidden、widths…）引擎不管。`prop` 都是 key，也认名字（Agent 用）。

**值**
- 名称存在 `props[名称属性的 key]`。日期 `{ start, end?, time? }`：`start` / `end` 是当地时间、不带时区的 `2026-11-15` 或 `2026-11-15T14:00`，写到几点时 `time: true`；给的值带 `Z` / `+08:00` 的换成引擎时区的当地时间。
  `created_at` / `updated_at` 是毫秒时间戳；自动编号读行的 `unique_id`。勾选没勾 = 不存（规范化 false 返回 null）。
- **百分比按 Notion**：数字格式 `percent` 显示时乘 100（0.5 → 50%），输入“50%”规范化成 0.5；汇总、底部汇总的 `percent_*` 算出 0–1，汇总没设格式时自动按百分比显示。（旧版存的是 0–100。）
- 显示：货币不是整数时两位小数（¥2.50、¥1,200）；勾选是“是”/“”；人没在人员表里的显示 id；关联对方库没加载时显示 id。
- 规范化：WASM 的 `normalize` 不新加选项（没有的选项抛错，网页先改表结构再写）；Rust 的 `normalize_with(.., create = true)` / `normalize_row` 把要新加的选项放在 `new_options` 里，由服务器写回表结构。
  新选项的 id 由属性 key + 名字算出来（`o` + 4 位十六进制，撞了往后挪），颜色按个数轮换。人按 `setPeople` 给的名字认（不分大小写），`user:` / `agent:` / `bot:` 开头的 id 直接认。关联写行 id、`#id` 或对方行的名称（对方库加载了才核对）。
- 改类型（`Engine::convert`）：同类的原样留着；多选变单选留第一个；别的先变成显示的文字再按新类型认（单选、多选新加选项），认不出清空。

**增量改动 `apply`**：`row` 整行换掉（同步来的）；`props`（也认 `set`）只改给了的格子，null / "" / [] 是清空；`deleted` 删行；没有的行 id 是加一行（排在最后）。返回改了的行，再加上关联自己时通过关联连着它们的行（它们的汇总、反向关联变了）。

**查询结果**多一个 `total`（符合条件的总行数，`ids` 是 limit / offset 那一段）。分组：
- 单选、状态、多选把所有选项按顺序列出来（没有行的也在，`count: 0`，除非 `hide_empty_groups`）；勾选是 `key: "true" / "false"`（“已勾选 / 未勾选”）；空值的组 `key: ""`、名字“无<属性名>”，排最后。
- 多选、人员、关联：一行落在它的每个值的组里。日期按 `group_date` 分段（key 如 `2026-11`、`2026-W47`、`today`）；数字按值；文字按原文。
- `sub_group_by` 的子分组在每组的 `groups` 里；视图有 `totals` 时每组也有 `totals`。底部汇总的属性或算法不认识时跳过（视图里留着的旧设置不报错）。

**筛选**
- 条件按类型：文字 eq / neq / contains / not_contains / starts_with / ends_with / gt… / empty / not_empty（不分大小写）；数字、编号 eq / neq / gt / gte / lt / lte（编号写 `T-12` 也认）；
  单选、状态 eq / neq（值可以是列表，任一命中），状态还有 `in_group`（todo / doing / done，或 未开始 / 进行中 / 已完成）；多选、人员、关联、文件 contains / not_contains（eq / neq 同义：任一命中）；
  勾选 checked / unchecked / eq；日期 eq / neq / gt / gte / lt / lte / is_within；公式、汇总按算出来的值的类型比。也认 is、before、after、包含 这些别名和旧的 `{ prop, op, value }`、直接给条件数组（= 且）。
- 日期按天比。日期范围：`eq` 是“这一天在范围里”，`is_within` 是“有重叠”，`gt` / `lt` 比开始那天。值可以写 today / tomorrow / yesterday / one_week_ago / one_week_from_now / one_month_ago / one_month_from_now（也认 今天 明天…）或具体日期；
  `is_within` 写 today、this_week（周一到周日）、this_month、this_year、past_week / past_month / past_year、next_week / next_month / next_year、past_N_days / next_N_days。
- 要值的条件没填值时不筛（UI 里刚加的条件）。属性、选项、人、条件写错报错（中文，列出能写的）。

**排序**：空的总在最后（升降序都是）；文字自然顺序（任务2 在 任务10 前），按码位，不按拼音；单选、状态按选项顺序；多选、人员、关联按显示的文字；勾选 未勾 < 勾了（不算空）；公式、汇总类型不一样时数 < 日期 < 文字 < 真假。值一样的保持原来的顺序。

**公式**
- 属性按名字引用（不分大小写、两边空格不算）；改名时服务器用 `formula::rename_prop` 改公式里的 `prop("旧名")`。也认全角的括号、逗号、冒号、问号、`×` `÷` `≠` `≥` `≤`，`=` 当 `==`。
- 解析错误带位置：`checkFormula` 的 `at` 是 UTF-16 下标（和 JS 字符串一致），错误信息开头是“公式第 n 个字”。给了 `key` 还查用没用到自己、成没成环。表结构里写错、成环的公式 / 汇总（包括跨库成环）值是空的，`schemaErrors()` 列出来。
- 算的时候出错（除以 0、类型不对、日期加数）这一格是空的。空值的规则同旧版：空 + 1 是空，`空 > 2` 是 false，文字和别的相加是拼起来。算出来的列表存成文字的列表（最多 200 个），文字最多 5000 字。
- 函数：旧版的全部，加上 Notion 2 的方法写法、`let` / `lets`、`[列表]`、`map / filter / find / findIndex / some / every`（`current`、`index`）、first / last / at / sort / reverse / unique / flat、split / repeat / startsWith / endsWith / padStart / padEnd、
  mod / sign / exp / ln / log10 / log2 / cbrt / pi / e / median、dateRange / dateStart / dateEnd / week / minute / timestamp / fromTimestamp（时间戳都是毫秒）。`day()` 是几号（同旧版），`weekday()` 周一 1 … 周日 7。
  `dateBetween` 两个方向对称（旧版日期 1 早于日期 2 时按月算会差一）。
- 缓存：公式、汇总、反向关联按行缓存；`apply` 只清改了的行，和读到主库别的行的列（关联自己的汇总、反向关联…）整列；`loadRelated` / `setSchema` 全清；用了 now() / today() 的公式到了下一分钟重算。
  跨库（汇总套汇总）最多 6 层。
- 汇总、底部汇总里“空”是 null、""、[]、false（**0 不算空**，旧版的汇总把 0 也当空）。

**WASM 接口**：约定里的方法都有，另外：`DbEngine.create(schema, opts?)` 的 `opts` 是 `{ id?（主库 id，关联自己时要）, now?（固定时钟，测试用）, tz?, people? }`；`createSync`（已经 init 过时）、`setPeople`、`setClock`、
`displayRow(rowId)`（一行每格的文字）、`schemaErrors()`。不固定时钟时每次调用前取浏览器的时间和时区。`computed` 除了公式、汇总、反向关联（对方行 id），也给创建 / 编辑时间（`{ start, time }`）、创建 / 编辑人、自动编号。
出错抛 `Error`（中文信息）。进出 WASM 都是 JSON 字符串：1 万行从网络来本来就是 JSON 文字，交给 Rust 解析一次最快，serde-wasm-bindgen 逐个字段过边界慢得多；
所以 `loadRows` / `loadRelated` / `apply` / `setSchema` / `query` 也可以直接给 JSON 字符串（fetch 的原文），省一次 `JSON.stringify`。默认导出是 wasm-bindgen 的 `init`。

**服务器用**：`Engine` 是 `Send`、不是 `Sync`（缓存用 RefCell），放进 Mutex。时钟默认是系统时间、UTC，服务器按工作区时区 `set_clock`。`api::JsonEngine` 是 WASM 用的那一层，测试拿它和 WASM 对照（`packages/db-wasm/test/golden.json`，由 `cargo test -p oad-db --test golden` 生成）。

**性能**（Apple M2，机器同时在编别的；1 万行 × 20 列，筛选（且 + 或 + 日期范围）+ 两列排序 + 按状态分组）：

| | Rust release | WASM（Node，含 JSON 来回） |
| --- | --- | --- |
| 加载 1 万行（3.5MB JSON） | 30ms | 90ms |
| 筛选 + 排序 + 分组 | 2.3ms | 3.0ms |
| 再加子分组 + 5 列汇总 | 8ms | 12ms |
| 按公式筛选排序：第一次 / 有缓存 | 10ms / 0.5ms | 14ms / 0.5ms |
| 改一格 + 重新查 | 1.3ms | 0.5ms |

（`cargo test -p oad-db --release --test bench -- --ignored --nocapture`、`node packages/db-wasm/test/bench.mjs`。）

**还没做**：中文按拼音排序；行内格式的文字；按状态组（todo / doing / done）分组、数字按区间分组；正则（test / match）、formatNumber、link / style 这些 Notion 函数；按钮（不存值，服务器管）。
