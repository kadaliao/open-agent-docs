//! MCP 的中英文：给 Agent 的固定文字、工具说明、参数说明、服务说明都在 [`MCP`] 这一张表里。
//! 中文原文就是键；英文 Agent（`PATCH /api/v1/agents/{id}` 设 `locale: "en"`）拿英文。
use oad_core::i18n::{CORE, DB_ENGINE, Lang, Table, t_in, translate};

/// 一句固定文字：`tr!(lang, "已建页面《{title}》", title = x)`。
macro_rules! tr {
    ($lang:expr, $zh:expr $(, $k:ident = $v:expr)* $(,)?) => {
        $crate::i18n::t($lang, $zh, &[$((stringify!($k), ($v).to_string().as_str())),*])
    };
}
pub(crate) use tr;

pub fn t(lang: Lang, zh: &str, args: &[(&str, &str)]) -> String {
    t_in(&[MCP, CORE, DB_ENGINE], lang, zh, args)
}

/// 已经拼好的中文（core 的错误信息等）翻成英文；对不上的原样。
pub fn tr_msg(lang: Lang, msg: &str) -> String {
    translate(&[MCP, CORE, DB_ENGINE], lang, msg)
}

/// 给模型的服务说明（initialize 的 instructions）。
pub const INSTRUCTIONS: &str = "这是 Open Agent Docs：人和 Agent 一起写的文档。\n\
- 页面正文按“块”组织（段落、标题、列表、表格……）。read_page 返回的正文里，每个顶层块前面有一行 ⟦块编号⟧，改的时候用这个编号指定块。\n\
- 改页面前先 read_page，拿到 base；edit_page / rewrite_page 时把 base 带回来。服务器在那个版本上改再合并，你读完之后别人新打的字不会被你覆盖。\n\
- 小改用 edit_page（按块替换、插入、删除，几步一起执行）；大改才用 rewrite_page（整页换，没变的块沿用编号，评论还挂着）。\n\
- 改一个列表、表格时，把整块的新 Markdown 给出来，服务器会沿用没变的项的编号。\n\
- 有人在评论里叫你时，用 my_notifications 取通知；回复和标成已解决用 comment。\n\
- 数据库（表格）：db_create 建库，db_schema 改列，db_add 加行，db_update 改格子，db_query 按条件查。属性和值都写名字（“状态”: “进行中”），不用写编号。\n\
  库里的每一行也是一个页面：行的正文照样用 read_page / edit_page（page_id 就是行 id），read_page 读行时先列属性。\n\
- 写给人看的内容用页面原来的语言。";

const INSTRUCTIONS_EN: &str = "This is Open Agent Docs: documents written together by people and agents.\n\
- Page bodies are made of blocks (paragraphs, headings, lists, tables...). In the body returned by read_page, every top-level block is preceded by a line ⟦block_id⟧; use that id to target the block when editing.\n\
- Before editing a page, call read_page to get base, and pass base back to edit_page / rewrite_page. The server applies your change on that version and merges, so text others typed after your read is not overwritten.\n\
- Use edit_page for small changes (replace, insert or delete blocks, several steps at once); use rewrite_page only for big rewrites (replaces the whole page; unchanged blocks keep their ids and comments stay attached).\n\
- When changing a list or a table, give the new Markdown of the whole block; the server keeps the ids of unchanged items.\n\
- When someone mentions you in a comment, fetch it with my_notifications; reply and resolve with comment.\n\
- Databases (tables): db_create creates one, db_schema changes columns, db_add adds rows, db_update changes cells, db_query queries by conditions. Write properties and values by name (\"Status\": \"In progress\"), not by id.\n\
  Every row of a database is also a page: read and edit a row's body with read_page / edit_page (page_id is the row id); read_page on a row lists its properties first.\n\
- Write content meant for people in the page's original language.";

pub static MCP: Table = &[
    (INSTRUCTIONS, INSTRUCTIONS_EN),
    // 工具说明
    (
        "全文搜索页面和数据库的行：中文按词、英文按词干，标题优先。返回页面编号（行就是行 id）、标题和命中的片段（**加粗**的是命中的词），命中在正文里时还给块编号。不知道页面编号时先用它，或者用 list_pages 看目录。",
        "Full-text search over pages and database rows (Chinese by word, English by stem; titles rank first). Returns page ids (for a row, the row id), titles and matching snippets (matched words in **bold**), plus the block id when the hit is in the body. Use it when you don't know a page id, or browse with list_pages.",
    ),
    (
        "页面目录：缩进表示层级，每行是标题、page_id 和更新时间。",
        "Page tree: indentation shows nesting; each line has the title, page_id and last update time.",
    ),
    (
        "读一个页面：标题、正文（每个顶层块前面一行 ⟦块编号⟧，块内容是 Markdown）、base（改的时候带回来）和没解决的评论概要。页面很长时分段读：结果末尾会告诉你下一段从哪块开始。",
        "Read a page: title, body (each top-level block preceded by a line ⟦block_id⟧, block content in Markdown), base (pass it back when editing) and a summary of unresolved comments. Long pages are read in parts: the end of the result tells you which block the next part starts at.",
    ),
    (
        "建一个新页面（可以带正文 Markdown），返回 page_id、网页链接和 base。",
        "Create a new page (optionally with a Markdown body). Returns page_id, the web link and base.",
    ),
    (
        "按块改页面。ops 依次执行、一次提交（一步出错就都不改）：replace{block, markdown} 换掉一块（markdown 可以是多块）；insert_after{block, markdown} / insert_before{block, markdown} 插入；append{markdown} 加到最后；delete{block} 删掉。block 是 read_page 里 ⟦⟧ 中的编号，只能是顶层块：改列表、表格里的一项就把整个列表 / 表格的新 Markdown 给出来。一定带上 read_page 给的 base。返回每一步涉及的块编号和新的 base（接着改就用新的）。",
        "Edit a page block by block. ops run in order and commit together (if one step fails nothing changes): replace{block, markdown} replaces a block (markdown may be several blocks); insert_after{block, markdown} / insert_before{block, markdown} insert; append{markdown} adds at the end; delete{block} deletes. block is the id inside ⟦⟧ from read_page and must be a top-level block: to change one item of a list or table, give the new Markdown of the whole list / table. Always pass the base from read_page. Returns the block ids touched by each step and the new base (use it for further edits).",
    ),
    (
        "整页换成新的 Markdown（大改时用；小改用 edit_page）。没变的块沿用编号，挂在上面的评论还在。带上 read_page 给的 base。可以顺便改标题。",
        "Replace the whole page with new Markdown (for big rewrites; use edit_page for small changes). Unchanged blocks keep their ids and their comments stay attached. Pass the base from read_page. Can also change the title.",
    ),
    (
        "改页面的标题、图标，或者移动页面（parent_id 给 \"root\" 移到最外层；after 给 \"first\" 放同级最前）。",
        "Change a page's title or icon, or move it (parent_id \"root\" moves it to the top level; after \"first\" puts it first among its siblings).",
    ),
    (
        "把页面（连同子页面）移到回收站。",
        "Move a page (with its subpages) to the trash.",
    ),
    (
        "列出页面的评论串：每串挂在哪一块（⟦块编号⟧ 和当时选中的原文）、谁说了什么、回复。默认只列没解决的。",
        "List a page's comment threads: which block each is attached to (⟦block_id⟧ and the text selected at the time), who said what, and replies. Only unresolved threads by default.",
    ),
    (
        "评论：给 page_id + body 写一条新评论（可以加 block 挂在某一块上）；给 thread_id + body 回复一串；给 thread_id + resolve: true 把这串标成已解决（false 重新打开），可以和 body 一起给（先回复再解决）。",
        "Comment: page_id + body writes a new comment (add block to attach it to a block); thread_id + body replies to a thread; thread_id + resolve: true marks the thread resolved (false reopens it), and can be combined with body (reply first, then resolve).",
    ),
    (
        "上传一个文件（图片、PDF、表格……，内容用 base64），存到这个工作区。返回能直接放进页面的 Markdown：图片是 ![](网址)，别的文件是单独一行的网址（网页里显示成文件卡片）。再用 edit_page / create_page 把它写进页面。",
        "Upload a file (image, PDF, spreadsheet..., content in base64) to this workspace. Returns Markdown you can put straight into a page: images are ![](url), other files are a URL on its own line (shown as a file card on the web). Then write it into a page with edit_page / create_page.",
    ),
    (
        "页面的历史版本：新的在前，每版有版本号、时间、种类（自动 / Agent 写入 / 恢复 / 手动）和改过的人；还列出每一版比上一版改了哪些块。要回到某一版用 restore_version。",
        "A page's version history, newest first: each version has its number, time, kind (auto / agent write / restore / manual) and who changed it, plus which blocks changed compared with the previous version. Use restore_version to go back to a version.",
    ),
    (
        "把页面恢复到某一版（version 是 page_history 里的版本号）。恢复是一次新的改动：现在的内容先存成一版，之后的历史都还在，随时能再恢复回来。",
        "Restore a page to a version (version is the number from page_history). Restoring is a new change: the current content is saved as a version first and later history is kept, so you can always restore back.",
    ),
    (
        "建一个数据库（表格）。返回 db_id 和表结构。之后用 db_add 加行、db_query 查。",
        "Create a database (table). Returns db_id and the schema. Then add rows with db_add and query with db_query.",
    ),
    (
        "改数据库的表结构（加列、改名、改类型、删列、改选项、改公式），属性写名字。一批改动一起提交。返回新的表结构和这次改动的 seq（撤销：db_schema(ops: [{op: \"undo\", seq}])，删掉的列和值都能放回来）。",
        "Change a database's schema (add, rename, retype or delete columns, change options or formulas), naming properties by name. A batch of changes commits together. Returns the new schema and the seq of this change (undo: db_schema(ops: [{op: \"undo\", seq}]); deleted columns and their values come back).",
    ),
    (
        "查数据库的行：按条件筛选、排序、搜索，返回 Markdown 表格（值是名字，不是编号），每行带 row_id，末尾是总数。不给条件就是全部（按库里的顺序）。行的正文用 read_page(row_id) 读。",
        "Query database rows: filter, sort and search; returns a Markdown table (values are names, not ids) with row_id on every row and the total at the end. No conditions means all rows (in database order). Read a row's body with read_page(row_id).",
    ),
    (
        "往数据库里加行。属性写名字，值也写名字（选项名、人名、行的名称、日期 2026-11-20、勾选 是 / 否）；单选、多选里没有的选项会新加上。返回新行的 row_id。",
        "Add rows to a database. Write properties by name and values by name too (option names, people's names, row names, dates like 2026-11-20, checkboxes as true / false); options missing from a select or multi-select are added. Returns the new row_ids.",
    ),
    (
        "改数据库里的行：`[{ row_id, set: { \"状态\": \"已完成\", \"价格\": 120 } }]`，属性和值写名字，null 清空。只改给了的格子（别人同时改别的格子不受影响）。公式、汇总这些算出来的不能改。",
        "Change database rows: `[{ row_id, set: { \"Status\": \"Done\", \"Price\": 120 } }]`, properties and values by name, null clears. Only the given cells change (others editing other cells at the same time are unaffected). Computed values such as formulas and rollups can't be changed.",
    ),
    (
        "取发给你的通知（比如有人在评论里叫你）。每条带一段说明：哪个页面、哪一段、评论内容、该怎么回复。处理完后下次调用时把最后一条的编号作为 after 传进来，它和之前的会标成已处理。可以用 wait 等新通知（最多 60 秒）。",
        "Fetch notifications for you (e.g. someone mentioned you in a comment). Each comes with a note: which page, which passage, the comment and how to reply. When done, pass the last notification's id as after on the next call to mark it and earlier ones handled. Use wait to wait for new notifications (up to 60 seconds).",
    ),
    // 参数说明
    (
        "要找的词，在标题和正文里按子串匹配（不分大小写）。",
        "Words to find, matched as substrings in titles and bodies (case-insensitive).",
    ),
    (
        "最多返回几页，默认 10。",
        "Maximum number of pages to return, default 10.",
    ),
    (
        "只列这个页面下面的子页面；不给就列整个工作区。",
        "Only list the subpages of this page; omit to list the whole workspace.",
    ),
    (
        "从这个块开始读（分段读长页面时用上次结果末尾给的编号）。",
        "Start reading at this block (for long pages, use the id given at the end of the previous result).",
    ),
    (
        "最多读几块，默认 200。",
        "Maximum number of blocks to read, default 200.",
    ),
    (
        "正文 Markdown（可以不给，建空页）。",
        "Body Markdown (optional; omit for an empty page).",
    ),
    (
        "父页面；不给就建在最外层。",
        "Parent page; omit to create at the top level.",
    ),
    (
        "放在同级里哪个页面后面；不给就放最后。",
        "Put it after this sibling page; omit to put it last.",
    ),
    (
        "把 block 换成 markdown（可以是多块）",
        "Replace block with markdown (may be several blocks)",
    ),
    ("在 block 后面插入 markdown", "Insert markdown after block"),
    ("在 block 前面插入 markdown", "Insert markdown before block"),
    (
        "在页面最后加上 markdown",
        "Append markdown at the end of the page",
    ),
    ("删掉 block", "Delete block"),
    (
        "操作：replace / insert_after / insert_before / append / delete。",
        "Operation: replace / insert_after / insert_before / append / delete.",
    ),
    (
        "块编号（read_page 里 ⟦⟧ 中的那串）；append 不用给。",
        "Block id (the string inside ⟦⟧ in read_page); not needed for append.",
    ),
    (
        "新内容（Markdown）；delete 不用给。",
        "New content (Markdown); not needed for delete.",
    ),
    (
        "read_page 给的 base，原样带回来。",
        "The base from read_page, passed back unchanged.",
    ),
    (
        "依次执行的操作；一步出错就都不改。",
        "Operations run in order; if one step fails nothing changes.",
    ),
    (
        "整页的新正文（Markdown）。",
        "The new body of the whole page (Markdown).",
    ),
    ("顺便改标题。", "Also change the title."),
    (
        "图标（一个 emoji）；给空字符串清掉。",
        "Icon (one emoji); an empty string clears it.",
    ),
    (
        "移到这个页面下面；给 \"root\" 移到最外层。",
        "Move under this page; \"root\" moves it to the top level.",
    ),
    (
        "移动时放在同级里这个页面后面；给 \"first\" 放最前；不给放最后。",
        "When moving, put it after this sibling page; \"first\" puts it first; omit to put it last.",
    ),
    (
        "也列出已解决的，默认只列没解决的。",
        "Also list resolved threads; by default only unresolved ones.",
    ),
    (
        "新评论：评论哪个页面。",
        "New comment: the page to comment on.",
    ),
    (
        "回复 / 解决：哪一串评论。",
        "Reply / resolve: which thread.",
    ),
    ("评论内容（Markdown）。", "Comment text (Markdown)."),
    (
        "新评论挂在哪一块上（块编号）；不给就是对整页的评论。",
        "Block to attach a new comment to (block id); omit for a comment on the whole page.",
    ),
    (
        "true：把这串标成已解决；false：重新打开。",
        "true: mark the thread resolved; false: reopen it.",
    ),
    (
        "文件名（带扩展名，比如 chart.png）。",
        "File name (with extension, e.g. chart.png).",
    ),
    ("文件内容，base64 编码。", "File content, base64-encoded."),
    (
        "最多列几版，默认 30。",
        "Maximum number of versions to list, default 30.",
    ),
    (
        "版本号（page_history 列出来的 #后面的数）。",
        "Version number (the number after # in page_history).",
    ),
    (
        "属性（列），按顺序：`[{ \"name\": \"状态\", \"type\": \"status\" }, { \"name\": \"价格\", \"type\": \"number\", \"format\": \"cny\" },\n{ \"name\": \"标签\", \"type\": \"multi_select\", \"options\": [\"急\", \"慢\"] }, { \"name\": \"总价\", \"type\": \"formula\", \"formula\": \"prop(\\\"价格\\\") * 2\" }]`。\n类型：title、text、number、select、multi_select、status、date、person、checkbox、url、email、phone、files、\ncreated_time、updated_time、created_by、updated_by、relation（给 db_id，twoway: true 是双向）、rollup（给 relation、prop、fn）、formula、unique_id、button。\n标题列自动有（叫“名称”）；想换名字就放一个 type 为 title 的。",
        "Properties (columns), in order: `[{ \"name\": \"Status\", \"type\": \"status\" }, { \"name\": \"Price\", \"type\": \"number\", \"format\": \"usd\" },\n{ \"name\": \"Tags\", \"type\": \"multi_select\", \"options\": [\"urgent\", \"later\"] }, { \"name\": \"Total\", \"type\": \"formula\", \"formula\": \"prop(\\\"Price\\\") * 2\" }]`.\nTypes: title, text, number, select, multi_select, status, date, person, checkbox, url, email, phone, files,\ncreated_time, updated_time, created_by, updated_by, relation (give db_id; twoway: true makes it two-way), rollup (give relation, prop, fn), formula, unique_id, button.\nThe title column exists automatically; to rename it, include one with type title.",
    ),
    (
        "建在哪个页面下面；不给就在最外层。",
        "Parent page; omit to create at the top level.",
    ),
    (
        "依次执行的改动，一步出错就都不改：\n`{op: \"add\", name, type, options?, formula?, format?, after?}`、`{op: \"rename\", prop, name}`、`{op: \"change_type\", prop, type}`（能转的值转过去）、\n`{op: \"delete\", prop}`、`{op: \"set_options\", prop, options}`、`{op: \"set_formula\", prop, formula}`、`{op: \"update\", prop, ...}`、`{op: \"move\", prop, after?}`、\n`{op: \"undo\", seq}`（撤销第 seq 次表结构改动，seq 是之前 db_schema 返回的）。prop 写属性名。",
        "Changes run in order; if one step fails nothing changes:\n`{op: \"add\", name, type, options?, formula?, format?, after?}`, `{op: \"rename\", prop, name}`, `{op: \"change_type\", prop, type}` (convertible values are converted),\n`{op: \"delete\", prop}`, `{op: \"set_options\", prop, options}`, `{op: \"set_formula\", prop, formula}`, `{op: \"update\", prop, ...}`, `{op: \"move\", prop, after?}`,\n`{op: \"undo\", seq}` (undo schema change number seq, as returned earlier by db_schema). prop is the property name.",
    ),
    (
        "筛选：`{ \"op\": \"and\", \"items\": [{ \"prop\": \"状态\", \"cond\": \"eq\", \"value\": \"进行中\" }, { \"prop\": \"价格\", \"cond\": \"gt\", \"value\": 100 }] }`，\n可以嵌套 `{ op: \"or\", items: [...] }`。cond：eq、neq、contains、not_contains、starts_with、gt、gte、lt、lte、empty、not_empty、is_within（past_week、next_month…）、checked…",
        "Filter: `{ \"op\": \"and\", \"items\": [{ \"prop\": \"Status\", \"cond\": \"eq\", \"value\": \"In progress\" }, { \"prop\": \"Price\", \"cond\": \"gt\", \"value\": 100 }] }`,\ncan nest `{ op: \"or\", items: [...] }`. cond: eq, neq, contains, not_contains, starts_with, gt, gte, lt, lte, empty, not_empty, is_within (past_week, next_month...), checked...",
    ),
    (
        "排序：`[{ \"prop\": \"价格\", \"dir\": \"desc\" }]`。",
        "Sorts: `[{ \"prop\": \"Price\", \"dir\": \"desc\" }]`.",
    ),
    (
        "从这个视图（名字或 id）的筛选、排序出发。",
        "Start from the filters and sorts of this view (name or id).",
    ),
    (
        "搜索：任一格包含这些字。",
        "Search: any cell contains this text.",
    ),
    (
        "最多几行，默认 50（最多 500）。",
        "Maximum rows, default 50 (at most 500).",
    ),
    ("跳过前几行（翻页）。", "Skip this many rows (paging)."),
    (
        "每行一个对象：`{ \"名称\": \"写周报\", \"状态\": \"进行中\", \"截止\": \"2026-11-20\", \"负责人\": \"张三\" }`；\n可以带 `body`（行的正文，Markdown）和 `after`（排在这一行后面）。",
        "One object per row: `{ \"Name\": \"Write weekly report\", \"Status\": \"In progress\", \"Due\": \"2026-11-20\", \"Owner\": \"Alice\" }`;\nmay include `body` (the row's body, Markdown) and `after` (place after this row).",
    ),
    (
        "要改的格子：`{ \"状态\": \"已完成\" }`；写 null 清空。",
        "Cells to change: `{ \"Status\": \"Done\" }`; null clears.",
    ),
    (
        "上次处理到的通知编号：编号不大于它的会标成已处理，只返回它之后的。不给就返回所有还没处理的。",
        "Id of the last notification you handled: notifications up to it are marked handled and only later ones are returned. Omit to get all unhandled ones.",
    ),
    (
        "没有新通知时最多等几秒（0 到 60），默认不等。",
        "Seconds to wait when there are no new notifications (0 to 60); default no waiting.",
    ),
    // 出错
    (
        "要用 Agent 的 token：Authorization: Bearer oad_...",
        "An agent token is required: Authorization: Bearer oad_...",
    ),
    ("找不到：{m}", "Not found: {m}"),
    ("没有权限：{m}", "Forbidden: {m}"),
    ("参数不对：{m}", "Bad arguments: {m}"),
    ("冲突：{m}", "Conflict: {m}"),
    ("已失效：{m}", "Gone: {m}"),
    (
        "去外部网站取东西失败：{m}",
        "Fetching from an external site failed: {m}",
    ),
    ("没有登录：{m}", "Not signed in: {m}"),
    (
        "服务器内部错误，稍后再试",
        "Internal server error, try again later",
    ),
    ("没有 Agent token", "No agent token"),
    (
        "base 不对：要原样用 read_page 给的 base",
        "Invalid base: pass back the base from read_page unchanged",
    ),
    ("Agent 已撤销", "The agent has been revoked"),
    ("第 {n} 步要给 block", "Step {n} needs block"),
    ("第 {n} 步要给 markdown", "Step {n} needs markdown"),
    (
        "给了 thread_id，还要给 body 或 resolve",
        "With thread_id, also give body or resolve",
    ),
    (
        "要给 page_id（新评论）或 thread_id（回复 / 解决）",
        "Give page_id (new comment) or thread_id (reply / resolve)",
    ),
    ("新评论要给 body", "A new comment needs body"),
    (
        "content_base64 不是合法的 base64",
        "content_base64 is not valid base64",
    ),
    (
        "props 里每一项要是 { name, type, ... }",
        "Each item in props must be { name, type, ... }",
    ),
    ("没有视图「{v}」", "No view \"{v}\""),
    ("rows 是空的", "rows is empty"),
    ("{row} 不是数据库里的行", "{row} is not a database row"),
    (
        "页面里没有块 ⟦{id}⟧（顶层块才行）",
        "The page has no block ⟦{id}⟧ (only top-level blocks work)",
    ),
    // 工具结果
    (
        "这是一个数据库（db_id: {id}，共 {seq} 次改动）。属性：\n{schema}查行用 db_query(db_id: \"{id}\")，加行 db_add，改行 db_update。\n\n",
        "This is a database (db_id: {id}, {seq} schema changes). Properties:\n{schema}Query rows with db_query(db_id: \"{id}\"), add rows with db_add, change rows with db_update.\n\n",
    ),
    (
        "已建页面《{title}》\npage_id: {id}\n网页：{url}\nbase: {base}",
        "Created page \"{title}\"\npage_id: {id}\nWeb: {url}\nbase: {base}",
    ),
    (
        "已重写，现在共 {n} 块。\nbase: {base}",
        "Rewritten; the page now has {n} blocks.\nbase: {base}",
    ),
    (
        "已更新：《{title}》 page_id: {id}",
        "Updated: \"{title}\" page_id: {id}",
    ),
    ("，图标 {icon}", ", icon {icon}"),
    ("，现在在 {parent} 下面", ", now under {parent}"),
    ("，现在在最外层", ", now at the top level"),
    (
        "已移到回收站：《{title}》和它的 {n} 个子页面",
        "Moved to trash: \"{title}\" and its {n} subpages",
    ),
    ("已移到回收站：《{title}》", "Moved to trash: \"{title}\""),
    ("已回复（comment_id: {id}）", "Replied (comment_id: {id})"),
    ("已标成已解决", "marked resolved"),
    ("已重新打开", "reopened"),
    ("，", ", "),
    ("{what}（thread_id: {id}）", "{what} (thread_id: {id})"),
    ("已评论（thread_id: {id}）", "Commented (thread_id: {id})"),
    ("{mime}，{size} 字节", "{mime}, {size} bytes"),
    ("，{w}×{h}", ", {w}×{h}"),
    (
        "已上传《{name}》（{info}）。放进页面用这段 Markdown：\n{md}",
        "Uploaded \"{name}\" ({info}). Put it into a page with this Markdown:\n{md}",
    ),
    (
        "已恢复到第 {v} 版（新的一版是 #{new}）\nbase: {base}",
        "Restored to version {v} (the new version is #{new})\nbase: {base}",
    ),
    (
        "已建数据库《{title}》\ndb_id: {id}\n网页：{url}\n属性：\n{schema}{notes}",
        "Created database \"{title}\"\ndb_id: {id}\nWeb: {url}\nProperties:\n{schema}{notes}",
    ),
    (
        "已撤销第 {s} 次改动（新的 seq: {seq}）\n",
        "Undid change {s} (new seq: {seq})\n",
    ),
    ("没有变化", "No changes"),
    (
        "已改表结构（{n} 处，seq: {seq}；撤销用这个 seq）\n",
        "Schema changed ({n} changes, seq: {seq}; undo with this seq)\n",
    ),
    (
        "{notes}《{title}》现在的属性：\n{schema}",
        "{notes}Current properties of \"{title}\":\n{schema}",
    ),
    (
        "已加 {n} 行，row_id：\n{list}",
        "Added {n} rows, row_id:\n{list}",
    ),
    (
        "没有变化（值和原来一样）",
        "No changes (the values are the same as before)",
    ),
    ("已改 {n} 格", "Changed {n} cells"),
    (
        "注意，这些公式 / 汇总现在算不出来：\n{lines}\n",
        "Note: these formulas / rollups can't be computed now:\n{lines}\n",
    ),
    // render
    ("（无标题）", "(untitled)"),
    ("（不知道是谁）", "(unknown)"),
    (
        "没有找到和「{q}」有关的页面。",
        "No pages found for \"{q}\".",
    ),
    ("找到 {n} 条：\n", "Found {n}:\n"),
    ("（数据库的行）", " (database row)"),
    ("无标题", "Untitled"),
    (
        "- 《{title}》{kind} page_id: {id}",
        "- \"{title}\"{kind} page_id: {id}",
    ),
    (" · 块 ⟦{b}⟧", " · block ⟦{b}⟧"),
    ("［数据库］", "[database] "),
    (
        "{indent}- {icon}{kind}{title} （page_id: {id}，更新于 {time}）\n",
        "{indent}- {icon}{kind}{title} (page_id: {id}, updated {time})\n",
    ),
    ("《{title}》下面的页面：\n", "Pages under \"{title}\":\n"),
    ("找不到页面 {id}。", "Page {id} not found."),
    ("（没有子页面）", "(no subpages)"),
    ("工作区里还没有页面。", "The workspace has no pages yet."),
    ("\n共 {n} 个页面。", "\n{n} pages in total."),
    (
        "# {title}\npage_id: {id} · 更新于 {time} · 共 {n} 块\nbase: {base}\n",
        "# {title}\npage_id: {id} · updated {time} · {n} blocks\nbase: {base}\n",
    ),
    ("\n（正文是空的）\n", "\n(the body is empty)\n"),
    (
        "\n（前面还有 {n} 块没显示）",
        "\n({n} earlier blocks not shown)",
    ),
    (
        "\n（后面还有 {n} 块。接着读：read_page(page_id: \"{id}\", from_block: \"{next}\")）\n",
        "\n({n} more blocks. Continue with: read_page(page_id: \"{id}\", from_block: \"{next}\"))\n",
    ),
    (
        "\n没解决的评论（{n} 串，详细用 list_comments）：\n",
        "\nUnresolved comments ({n} threads; details via list_comments):\n",
    ),
    (
        "- thread_id: {id} · {at} · {by}：{body}{more}\n",
        "- thread_id: {id} · {at} · {by}: {body}{more}\n",
    ),
    ("（另有 {n} 条回复）", " ({n} more replies)"),
    ("挂在 ⟦{b}⟧", "on ⟦{b}⟧"),
    ("挂在页面上", "on the page"),
    ("对整页", "on the whole page"),
    ("已改好（{n} 步）：\n", "Done ({n} steps):\n"),
    ("替换 ⟦{b}⟧ → {ids}", "Replaced ⟦{b}⟧ → {ids}"),
    ("在 ⟦{b}⟧ 后面插入 {ids}", "Inserted {ids} after ⟦{b}⟧"),
    ("在 ⟦{b}⟧ 前面插入 {ids}", "Inserted {ids} before ⟦{b}⟧"),
    ("在最后加上 {ids}", "Appended {ids}"),
    ("删掉 ⟦{b}⟧", "Deleted ⟦{b}⟧"),
    ("新的 base: {base}", "New base: {base}"),
    (
        "《{title}》没有没解决的评论。",
        "\"{title}\" has no unresolved comments.",
    ),
    ("《{title}》没有评论。", "\"{title}\" has no comments."),
    (
        "《{title}》的评论（没解决的 {n} 串）：\n",
        "Unresolved comments on \"{title}\" ({n} threads):\n",
    ),
    (
        "《{title}》的评论（{n} 串）：\n",
        "Comments on \"{title}\" ({n} threads):\n",
    ),
    ("已解决", "resolved"),
    ("没解决", "unresolved"),
    (
        "\n【thread_id: {id}】{at} · {state}\n",
        "\n[thread_id: {id}] {at} · {state}\n",
    ),
    ("  - （已删除）\n", "  - (deleted)\n"),
    (
        "  - {who}（{time}）：{body}\n",
        "  - {who} ({time}): {body}\n",
    ),
    ("没有新通知。", "No new notifications."),
    ("{n} 条通知：\n", "{n} notifications:\n"),
    (
        "\n【通知 {id}】（给 {to}，{time}）\n",
        "\n[Notification {id}] (to {to}, {time})\n",
    ),
    (
        "\n处理完后调用 my_notifications(after: {last}) 把这些标成已处理并取新的。",
        "\nWhen done, call my_notifications(after: {last}) to mark these handled and fetch new ones.",
    ),
    (
        "《{title}》（db_id: {id}）\n\n",
        "\"{title}\" (db_id: {id})\n\n",
    ),
    ("没有符合条件的行。\n", "No matching rows.\n"),
    ("这一段没有行。\n", "No rows in this range.\n"),
    ("\n共 {n} 行\n", "\n{n} rows in total\n"),
    (
        "\n共 {total} 行，这里是第 {from}–{to} 行",
        "\n{total} rows in total; these are rows {from}–{to}",
    ),
    ("。下一段：offset: {end}", ". Next page: offset: {end}"),
    (
        "这是数据库《{title}》（db_id: {id}）里的一行。属性：\n",
        "This is a row of database \"{title}\" (db_id: {id}). Properties:\n",
    ),
    ("（都是空的）\n", "(all empty)\n"),
    (
        "改属性用 db_update；下面是这一行的正文。\n\n",
        "Change properties with db_update; the row's body follows.\n\n",
    ),
    (
        "《{title}》（page_id: {id}）的历史，新的在前：\n",
        "History of \"{title}\" (page_id: {id}), newest first:\n",
    ),
    ("还没有存过版本。\n", "No versions saved yet.\n"),
    ("Agent 写入", "agent write"),
    ("恢复", "restore"),
    ("手动", "manual"),
    ("自动", "auto"),
    (" · 「{label}」", " · \"{label}\""),
    (
        "（加 {a} 块、改 {c} 块、删 {r} 块）",
        " (blocks: {a} added, {c} changed, {r} removed)",
    ),
];

/// 包一层引号（不进表：这种只有变量的模板反查时会误认别的句子）。
pub fn quote(lang: Lang, s: &str) -> String {
    match lang {
        Lang::Zh => format!("「{s}」"),
        Lang::En => format!(" \"{s}\""),
    }
}

/// 列举的分隔：中文“、”，英文“, ”。
pub fn list_sep(lang: Lang) -> &'static str {
    match lang {
        Lang::Zh => "、",
        Lang::En => ", ",
    }
}

/// “名字（Agent）”。
pub fn agent_name(lang: Lang, name: &str) -> String {
    match lang {
        Lang::Zh => format!("{name}（Agent）"),
        Lang::En => format!("{name} (agent)"),
    }
}

/// “- 名字：值”。
pub fn kv_line(lang: Lang, k: &str, v: &str) -> String {
    match lang {
        Lang::Zh => format!("- {k}：{v}\n"),
        Lang::En => format!("- {k}: {v}\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeSet, HashSet};

    pub(crate) fn is_cjk(c: char) -> bool {
        matches!(c as u32, 0x3000..=0x303F | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xFF00..=0xFFEF)
    }

    fn holes(s: &str) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut rest = s;
        while let Some(i) = rest.find('{') {
            let after = &rest[i + 1..];
            match after.find('}') {
                Some(j) if j > 0 && after[..j].chars().all(|c| c.is_alphanumeric() || c == '_') => {
                    out.insert(after[..j].to_string());
                    rest = &after[j + 1..];
                }
                _ => rest = after,
            }
        }
        out
    }

    #[test]
    fn table_is_sound() {
        let mut seen = HashSet::new();
        for (zh, en) in MCP {
            assert!(seen.insert(*zh), "重复的键：{zh}");
            assert_eq!(holes(zh), holes(en), "变量对不上：{zh} / {en}");
            assert!(!en.chars().any(is_cjk), "英文里有中文：{en}");
        }
    }

    #[test]
    fn forward_and_back() {
        assert_eq!(
            tr!(
                Lang::En,
                "已建页面《{title}》\npage_id: {id}\n网页：{url}\nbase: {base}",
                title = "A",
                id = 1,
                url = "u",
                base = "b"
            ),
            "Created page \"A\"\npage_id: 1\nWeb: u\nbase: b"
        );
        assert_eq!(tr!(Lang::Zh, "已改 {n} 格", n = 3), "已改 3 格");
        assert_eq!(tr_msg(Lang::En, "第 2 步要给 block"), "Step 2 needs block");
        assert_eq!(
            tr_msg(Lang::En, "页面里没有块 ⟦x⟧（顶层块才行）"),
            "The page has no block ⟦x⟧ (only top-level blocks work)"
        );
    }
}
