//! 双语（中文 / 英文）。中文原文就是键：`t(lang, "页面 {id} 不存在", &[("id", x)])` 英文时查表换成英文，查不到用中文。
//! 已经拼好的中文（错误信息）也能翻：[`translate`] 按模板反过来认出变量再填进英文模板；套在里面的一句也会翻。
//! 每个 crate 的字符串表放在它自己的一个地方（这里是 [`CORE`]，服务器、MCP 各有一份）。
use crate::{Error, Result};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Lang {
    #[default]
    Zh,
    En,
}

impl Lang {
    pub fn parse(s: &str) -> Option<Lang> {
        let s = s.trim().to_ascii_lowercase();
        if s == "zh" || s.starts_with("zh-") || s.starts_with("zh_") {
            Some(Lang::Zh)
        } else if s == "en" || s.starts_with("en-") || s.starts_with("en_") {
            Some(Lang::En)
        } else {
            None
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Lang::Zh => "zh",
            Lang::En => "en",
        }
    }

    /// `Accept-Language`：按权重取第一个认识的（中文、英文）；都不认识就是 None。
    pub fn from_accept(header: &str) -> Option<Lang> {
        let mut items: Vec<(f32, Lang)> = header
            .split(',')
            .filter_map(|p| {
                let mut it = p.split(';');
                let lang = Lang::parse(it.next()?)?;
                let q = it
                    .find_map(|x| x.trim().strip_prefix("q="))
                    .and_then(|q| q.parse::<f32>().ok())
                    .unwrap_or(1.0);
                Some((q, lang))
            })
            .collect();
        items.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        items.first().map(|x| x.1)
    }
}

/// 存进偏好的语言：zh / en / 清掉。
pub fn check_locale(l: Option<String>) -> Result<Option<String>> {
    match l.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(s) => match Lang::parse(s) {
            Some(x) => Ok(Some(x.as_str().to_string())),
            None => Err(Error::BadRequest(format!(
                "不支持的语言 {s}（只能是 zh、en）"
            ))),
        },
    }
}

/// 一张表：(中文模板, 英文模板)，变量写成 `{名字}`。
pub type Table = &'static [(&'static str, &'static str)];

fn find(tables: &[Table], zh: &str) -> Option<&'static str> {
    tables
        .iter()
        .flat_map(|t| t.iter())
        .find(|(z, _)| *z == zh)
        .map(|(_, e)| *e)
}

/// 填变量：`{名字}` 换成值；没给的原样留着。
pub fn fill(tpl: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(tpl.len() + 16);
    let mut rest = tpl;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match after.find('}') {
            Some(j) if after[..j].chars().all(|c| c.is_alphanumeric() || c == '_') && j > 0 => {
                let name = &after[..j];
                match args.iter().find(|(k, _)| *k == name) {
                    Some((_, v)) => out.push_str(v),
                    None => {
                        out.push('{');
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = &after[j + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// 按语言出一句话：中文直接填变量，英文查表（查不到用中文）。
pub fn t_in(tables: &[Table], lang: Lang, zh: &str, args: &[(&str, &str)]) -> String {
    match lang {
        Lang::Zh => fill(zh, args),
        Lang::En => fill(find(tables, zh).unwrap_or(zh), args),
    }
}

/// 模板拆成（字面量, 变量名）交替的片段。
fn pieces(tpl: &str) -> (Vec<&str>, Vec<&str>) {
    let mut lits = Vec::new();
    let mut names = Vec::new();
    let mut rest = tpl;
    loop {
        match rest
            .find('{')
            .and_then(|i| rest[i..].find('}').map(|j| (i, i + j)))
        {
            Some((i, j))
                if j > i + 1
                    && rest[i + 1..j]
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_') =>
            {
                lits.push(&rest[..i]);
                names.push(&rest[i + 1..j]);
                rest = &rest[j + 1..];
            }
            _ => {
                lits.push(rest);
                return (lits, names);
            }
        }
    }
}

/// 一句中文对上模板：返回变量的值。
fn match_tpl<'a>(tpl: &str, msg: &'a str) -> Option<Vec<(String, &'a str)>> {
    let (lits, names) = pieces(tpl);
    if names.is_empty() {
        return (tpl == msg).then(Vec::new);
    }
    let mut rest = msg.strip_prefix(lits[0])?;
    let mut vals = Vec::new();
    for (k, name) in names.iter().enumerate() {
        let next = lits[k + 1];
        let v = if k + 1 == names.len() {
            let v = rest.strip_suffix(next)?;
            rest = "";
            v
        } else if next.is_empty() {
            return None;
        } else {
            let i = rest.find(next)?;
            let v = &rest[..i];
            rest = &rest[i + next.len()..];
            v
        };
        vals.push((name.to_string(), v));
    }
    rest.is_empty().then_some(vals)
}

/// 已经拼好的中文按表翻成英文（变量里套着的一句也翻）；对不上的原样返回。
pub fn translate(tables: &[Table], lang: Lang, msg: &str) -> String {
    if lang == Lang::Zh || msg.is_empty() {
        return msg.to_string();
    }
    translate_depth(tables, msg, 0)
}

fn translate_depth(tables: &[Table], msg: &str, depth: u8) -> String {
    if let Some(e) = find(tables, msg) {
        return e.to_string();
    }
    // 变量多的模板先试（更具体）
    // (模板里字面量的长度, 英文模板, 变量)
    type Best<'a> = Option<(usize, &'static str, Vec<(String, &'a str)>)>;
    let mut best: Best = None;
    for (zh, en) in tables.iter().flat_map(|t| t.iter()) {
        if !zh.contains('{') {
            continue;
        }
        if let Some(vals) = match_tpl(zh, msg) {
            let lit_len = zh.len() - vals.iter().map(|(n, _)| n.len() + 2).sum::<usize>();
            if best.as_ref().is_none_or(|b| lit_len > b.0) {
                best = Some((lit_len, en, vals));
            }
        }
    }
    match best {
        Some((_, en, vals)) => {
            let vals: Vec<(String, String)> = vals
                .into_iter()
                .map(|(n, v)| {
                    let v = if depth < 3 {
                        translate_depth(tables, v, depth + 1)
                    } else {
                        v.to_string()
                    };
                    (n, v)
                })
                .collect();
            let args: Vec<(&str, &str)> =
                vals.iter().map(|(n, v)| (n.as_str(), v.as_str())).collect();
            fill(en, &args)
        }
        None => msg.to_string(),
    }
}

impl crate::Core {
    /// 这个人 / Agent 设的语言（没设是 None）。
    pub async fn locale_of(&self, user_id: &str) -> Option<Lang> {
        self.storage
            .user_prefs(user_id)
            .await
            .ok()?
            .locale
            .as_deref()
            .and_then(Lang::parse)
    }

    /// 这个人 / Agent 的语言：设过的，没设过用中文。
    pub async fn lang_of(&self, user_id: &str) -> Lang {
        self.locale_of(user_id).await.unwrap_or_default()
    }

    /// oad-core 的一句话。
    pub fn t(&self, lang: Lang, zh: &str, args: &[(&str, &str)]) -> String {
        t_in(&[CORE], lang, zh, args)
    }
}

/// oad-core 自己的话（错误信息、通知），也包括 oad-doc 的错误。
pub static CORE: Table = &[
    // 页面、块
    ("页面 {id} 不存在", "Page {id} not found"),
    // 表结构给 Agent 看（dbengine::schema_text）
    (
        "：《{name}》那边关联的反向，自动算",
        ": reverse side of the relation in \"{name}\", computed",
    ),
    ("：指向《{name}》的行", ": rows of \"{name}\""),
    (
        "：沿「{rel}」取「{prop}」，{agg}",
        ": \"{prop}\" via \"{rel}\", {agg}",
    ),
    ("，格式 {format}", ", format {format}"),
    (" · 只读", " · read-only"),
    ("页面不存在", "Page not found"),
    ("父页面 {parent} 不存在", "Parent page {parent} not found"),
    ("找不到块 {id}", "Block {id} not found"),
    ("第 {n} 步：{why}", "Step {n}: {why}"),
    ("ops 是空的", "ops is empty"),
    (
        "after={id} 不是同级页面",
        "after={id} is not a sibling page",
    ),
    (
        "不能移到自己或子页面下面",
        "Can't move a page under itself or its subpages",
    ),
    ("只读权限", "Read-only access"),
    ("不是这个工作区的成员", "Not a member of this workspace"),
    ("工作区 {id} 不存在", "Workspace {id} not found"),
    // 协同、版本
    ("服务器正在关闭", "Server is shutting down"),
    (
        "页面 {id} 加载或处理失败",
        "Failed to load or process page {id}",
    ),
    ("页面 {id} 加载失败", "Failed to load page {id}"),
    (
        "base 要和写入者一起给",
        "base must be given together with a writer",
    ),
    ("base 版本不对：{e}", "Invalid base version: {e}"),
    ("存版本失败", "Failed to save version"),
    ("没有第 {v} 版", "Version {v} not found"),
    ("读不出这一版：{e}", "Can't read this version: {e}"),
    ("恢复到第 {v} 版", "Restored version {v}"),
    // oad-doc
    ("增量格式不对：{e}", "Malformed update: {e}"),
    ("合并增量失败：{e}", "Failed to apply update: {e}"),
    ("版本不对：{m}", "Invalid version: {m}"),
    ("时钟超出范围", "clock out of range"),
    ("删除集超出范围", "delete set out of range"),
    // 错误类型
    ("存储出错：{e}", "Storage error: {e}"),
    ("内部错误：{e}", "Internal error: {e}"),
    ("写线程已退出", "Writer thread has exited"),
    ("建 {path}：{e}", "Create {path}: {e}"),
    // 登录、成员、邀请
    (
        "当前登录方式不支持密码登录",
        "Password sign-in isn't supported by the current auth mode",
    ),
    ("邮箱或密码不对", "Wrong email or password"),
    ("邮箱格式不对", "Invalid email address"),
    ("密码至少 8 位", "Password must be at least 8 characters"),
    (
        "邮箱 {email} 已经注册过",
        "Email {email} is already registered",
    ),
    ("名字不能是空的", "Name can't be empty"),
    ("Agent 不能管成员", "Agents can't manage members"),
    (
        "只有管理员能管成员和邀请",
        "Only admins can manage members and invites",
    ),
    (
        "不能邀请比自己权限高的角色",
        "Can't invite a role higher than your own",
    ),
    (
        "expires_in 要是正数（秒）",
        "expires_in must be a positive number (seconds)",
    ),
    ("没有这个邀请", "Invite not found"),
    ("邀请链接无效", "Invalid invite link"),
    ("邀请链接已失效", "Invite link has expired"),
    ("Agent 不能接受邀请", "Agents can't accept invites"),
    (
        "当前登录方式不支持注册",
        "Sign-up isn't supported by the current auth mode",
    ),
    ("这个站点不开放注册", "Sign-up is closed on this site"),
    ("要凭邀请链接注册", "Sign-up requires an invite link"),
    ("只有所有者能改所有者", "Only owners can change owners"),
    (
        "不能给比自己权限高的角色",
        "Can't grant a role higher than your own",
    ),
    (
        "工作区至少要有一个所有者",
        "A workspace needs at least one owner",
    ),
    ("只有所有者能移出所有者", "Only owners can remove owners"),
    (
        "Agent 在 Agent 设置里撤销",
        "Revoke agents in agent settings",
    ),
    (
        "不认识的时区 {tz}（要写 IANA 名字，如 Asia/Shanghai）",
        "Unknown time zone {tz} (use an IANA name such as Asia/Shanghai)",
    ),
    ("头像要是网址", "Avatar must be a URL"),
    (
        "当前登录方式不支持改密码",
        "Changing the password isn't supported by the current auth mode",
    ),
    ("现在的密码不对", "Current password is wrong"),
    (
        "不支持的语言 {s}（只能是 zh、en）",
        "Unsupported language {s} (must be zh or en)",
    ),
    // Agent
    ("名字要 1 到 40 个字", "Name must be 1 to 40 characters"),
    (
        "名字里不能有空白和 @ [ ] ( ) < > 引号 逗号（@ 的时候要用）",
        "Name can't contain whitespace, @ [ ] ( ) < >, quotes or commas (they're used for @mentions)",
    ),
    (
        "只有 admin 权限的 Agent 能管 Agent",
        "Only agents with admin scope can manage agents",
    ),
    ("Agent 不能管自己", "An agent can't manage itself"),
    ("Agent {id} 不存在", "Agent {id} not found"),
    ("Agent 不存在", "Agent not found"),
    ("Agent 已撤销", "Agent has been revoked"),
    (
        "这个工作区已经有叫 {name} 的 Agent",
        "This workspace already has an agent named {name}",
    ),
    ("建 Agent 失败", "Failed to create agent"),
    // 通知、webhook
    ("webhook 地址不对：{e}", "Invalid webhook URL: {e}"),
    ("webhook 只能是 http(s)", "webhook must be http(s)"),
    ("webhook 地址不允许：{e}", "webhook URL not allowed: {e}"),
    ("没有设 webhook", "No webhook set"),
    (
        "只有 Agent 能收 Agent 通知",
        "Only agents can receive agent notifications",
    ),
    ("不允许连接的地址：{e}", "Address not allowed: {e}"),
    ("{host}: 解析不出地址", "{host}: can't resolve address"),
    // 评论
    (
        "notify 只能是 false 或 [\"agent:<id>\", ...]",
        "notify must be false or [\"agent:<id>\", ...]",
    ),
    (
        "评论串 {thread_id} 不存在",
        "Comment thread {thread_id} not found",
    ),
    ("评论 {id} 不存在", "Comment {id} not found"),
    ("只能改自己的评论", "You can only edit your own comments"),
    ("只能删自己的评论", "You can only delete your own comments"),
    ("评论不能是空的", "Comment can't be empty"),
    ("评论太长了", "Comment is too long"),
    // 写给 Agent 的通知（prompt_for）
    ("（无标题）", "(Untitled)"),
    (
        "你在 Open Agent Docs 里被叫到了。{who} 在页面《{title}》（page_id: {page}）的块 ⟦{block}⟧ 上写了评论：",
        "You were mentioned in Open Agent Docs. {who} commented on block ⟦{block}⟧ of page \"{title}\" (page_id: {page}):",
    ),
    (
        "你在 Open Agent Docs 里被叫到了。{who} 在页面《{title}》（page_id: {page}）的块 ⟦{block}⟧ 上回复了评论：",
        "You were mentioned in Open Agent Docs. {who} replied to a comment on block ⟦{block}⟧ of page \"{title}\" (page_id: {page}):",
    ),
    (
        "你在 Open Agent Docs 里被叫到了。{who} 在页面《{title}》（page_id: {page}）里写了评论：",
        "You were mentioned in Open Agent Docs. {who} commented on page \"{title}\" (page_id: {page}):",
    ),
    (
        "你在 Open Agent Docs 里被叫到了。{who} 在页面《{title}》（page_id: {page}）里回复了评论：",
        "You were mentioned in Open Agent Docs. {who} replied to a comment on page \"{title}\" (page_id: {page}):",
    ),
    ("选中的原文：「{quote}」", "Selected text: \"{quote}\""),
    ("评论：{body}", "Comment: {body}"),
    (
        "先用 read_page(page_id: \"{page}\") 看上下文，用 list_comments 看这串之前的对话。\
回复评论用 comment(thread_id: \"{thread}\", body: \"...\")；要改页面用 edit_page(page_id, base, ops)（base 来自 read_page）；\
处理完用 comment(thread_id: \"{thread}\", resolve: true) 把这串标成已解决。",
        "First call read_page(page_id: \"{page}\") for context, and list_comments to see the earlier messages in this thread. \
Reply with comment(thread_id: \"{thread}\", body: \"...\"); to edit the page use edit_page(page_id, base, ops) (base comes from read_page); \
when done, mark the thread resolved with comment(thread_id: \"{thread}\", resolve: true).",
    ),
    (
        "先用 read_page(page_id: \"{page}\") 看上下文。\
回复评论用 comment(thread_id: \"{thread}\", body: \"...\")；要改页面用 edit_page(page_id, base, ops)（base 来自 read_page）；\
处理完用 comment(thread_id: \"{thread}\", resolve: true) 把这串标成已解决。",
        "First call read_page(page_id: \"{page}\") for context. \
Reply with comment(thread_id: \"{thread}\", body: \"...\"); to edit the page use edit_page(page_id, base, ops) (base comes from read_page); \
when done, mark the thread resolved with comment(thread_id: \"{thread}\", resolve: true).",
    ),
    // 文件、链接解析
    ("不合法的内容键：{key}", "Invalid content key: {key}"),
    ("文件太大（最多 {n} MB）", "File too large (max {n} MB)"),
    ("文件是空的", "File is empty"),
    ("内容不见了", "Content is missing"),
    ("文件不存在", "File not found"),
    ("网址不对", "Invalid URL"),
    ("解析失败", "Failed to fetch preview"),
    ("没有开链接解析", "Link previews are not enabled"),
    // 搜索、分享、回收站
    ("要给搜索词", "A search query is required"),
    (
        "kind 只能是 page、row、all，不是 {k}",
        "kind must be page, row or all, not {k}",
    ),
    ("这个页面没有分享", "This page isn't shared"),
    (
        "库里的一行不能单独分享，分享整个库",
        "A database row can't be shared on its own; share the whole database",
    ),
    (
        "分享链接无效或已关闭",
        "Share link is invalid or has been turned off",
    ),
    ("这个页面不在回收站里", "This page isn't in the trash"),
    (
        "这一行所在的库也在回收站里，先恢复库",
        "This row's database is also in the trash; restore the database first",
    ),
    (
        "要先移到回收站才能彻底删除",
        "Move it to the trash before deleting it permanently",
    ),
    // 数据库
    ("{db_id} 不是数据库", "{db_id} is not a database"),
    ("数据库 {db_id} 不存在", "Database {db_id} not found"),
    ("数据库不存在", "Database not found"),
    ("没有要加的行", "No rows to add"),
    ("一次最多加 1 万行", "At most 10,000 rows per request"),
    ("第 {n} 行：{why}", "Row {n}: {why}"),
    ("没有按钮「{key}」", "No button \"{key}\""),
    ("库里没有行 {row_id}", "No row {row_id} in this database"),
    (
        "after 不是这个库里的行",
        "after is not a row in this database",
    ),
    (
        "第 {seq} 次没有格子改动",
        "Change #{seq} has no cell changes",
    ),
    ("加了一行", "Added a row"),
    ("删了一行", "Deleted a row"),
    ("至少要有一个视图", "At least one view is required"),
    ("视图要是对象", "Each view must be an object"),
    ("视图要是数组", "Views must be an array"),
    ("没有视图 {view_id}", "View {view_id} not found"),
    ("要是对象", "Must be an object"),
    ("最后一个视图不能删", "Can't delete the last view"),
    ("视图不对：{e}", "Invalid view: {e}"),
    ("sorts 不对：{e}", "Invalid sorts: {e}"),
    (
        "不认识的视图类型 {t}（只能是 {list}）",
        "Unknown view type {t} (must be one of {list})",
    ),
    ("CSV 读不了：{e}", "Can't read CSV: {e}"),
    ("CSV 是空的", "CSV is empty"),
    ("最多 {n} 列", "At most {n} columns"),
    ("最多导入 {n} 行", "At most {n} rows can be imported"),
    // 表结构
    ("表结构不对：{e}", "Invalid schema: {e}"),
    ("表结构里没有标题", "Schema has no title"),
    (
        "每个属性要是 { name, type, ... }",
        "Each property must be { name, type, ... }",
    ),
    ("不认识的类型「{t}」", "Unknown type \"{t}\""),
    ("属性要有名字", "Property needs a name"),
    (
        "有两个叫「{name}」的属性",
        "Two properties are named \"{name}\"",
    ),
    ("「{name}」的类型不认识", "Unknown type for \"{name}\""),
    ("只能有一个标题属性", "Only one title property is allowed"),
    ("type 要是字符串", "type must be a string"),
    (
        "不认识的类型「{s}」，能用的：{list}",
        "Unknown type \"{s}\"; available: {list}",
    ),
    ("名字要是字符串", "name must be a string"),
    (
        "已经有叫「{name}」的属性了",
        "A property named \"{name}\" already exists",
    ),
    (
        "要给 prop（属性的 key 或名字）",
        "prop is required (property key or name)",
    ),
    (
        "没有属性「{r}」，现在有：{list}",
        "No property \"{r}\"; existing: {list}",
    ),
    ("options 要是数组", "options must be an array"),
    (
        "选项要是名字或 { name, color? }",
        "Each option must be a name or { name, color? }",
    ),
    ("选项要有名字", "Option needs a name"),
    ("formula 要是字符串", "formula must be a string"),
    ("「{name}」不是关联", "\"{name}\" is not a relation"),
    (
        "关联的库里没有属性「{t}」",
        "The related database has no property \"{t}\"",
    ),
    ("不认识的汇总方式「{f}」", "Unknown rollup function \"{f}\""),
    ("没有要改的", "Nothing to change"),
    ("每个改动都要是一个对象", "Each change must be an object"),
    ("最多 {n} 个属性", "At most {n} properties"),
    (
        "已经有标题了，只能有一个",
        "There's already a title; only one is allowed",
    ),
    ("标题不能删", "The title can't be deleted"),
    ("标题总在第一个", "The title always comes first"),
    ("要给 type", "type is required"),
    ("标题的类型不能改", "The title's type can't be changed"),
    (
        "「{name}」是另一个库的关联在这边的一半，不能改类型（不要了就删掉）",
        "\"{name}\" is this side of another database's relation; its type can't be changed (delete it if you don't need it)",
    ),
    (
        "「{name}」不是单选、多选或状态，没有选项",
        "\"{name}\" is not a select, multi-select or status; it has no options",
    ),
    ("「{name}」不是公式", "\"{name}\" is not a formula"),
    ("每个改动要有 op", "Each change needs an op"),
    (
        "不认识的改动 {k}（能用的：add、rename、change_type、delete、set_options、set_formula、update、move）",
        "Unknown change {k} (available: add, rename, change_type, delete, set_options, set_formula, update, move)",
    ),
    ("「{name}」：{msg}", "\"{name}\": {msg}"),
    (
        "{msg}（第 {i} 个字附近：{src}）",
        "{msg} (near character {i}: {src})",
    ),
    ("没有第 {seq} 次表结构改动", "No schema change #{seq}"),
    // 表结构改动的说明（schema_summary）
    ("撤销了第 {seq} 次改动", "Undid change #{seq}"),
    ("加了「{name}」", "Added \"{name}\""),
    ("删了「{name}」", "Deleted \"{name}\""),
    (
        "「{from}」改名为「{name}」",
        "Renamed \"{from}\" to \"{name}\"",
    ),
    ("「{name}」改了类型", "Changed the type of \"{name}\""),
    ("改了「{name}」的选项", "Changed the options of \"{name}\""),
    ("改了「{name}」的公式", "Changed the formula of \"{name}\""),
    ("挪了「{name}」", "Moved \"{name}\""),
    ("改了「{name}」", "Changed \"{name}\""),
    ("加了选项", "Added options"),
    ("改了表结构", "Changed the schema"),
    ("，", ", "),
];

/// oad-db 的话的英文（DbError、公式错误）：oad-db 只出中文，用到它的地方带上这张表翻。
pub static DB_ENGINE: Table = &[
    // 汇总、引用
    (
        "汇总还没设置要沿哪个关联、取哪个属性",
        "Rollup isn't set up yet: choose a relation and a property",
    ),
    (
        "汇总用的关联属性不存在了",
        "The rollup's relation property no longer exists",
    ),
    (
        "关联的库里没有属性「{p}」了",
        "The related database no longer has property \"{p}\"",
    ),
    (
        "没有属性「{name}」，现在有：{list}",
        "No property \"{name}\"; existing: {list}",
    ),
    ("没有属性「{key}」", "No property \"{key}\""),
    ("公式不能用到它自己", "A formula can't refer to itself"),
    (
        "「{name}」和别的公式 / 汇总互相引用成了环",
        "\"{name}\" forms a cycle with other formulas / rollups",
    ),
    // 公式
    ("公式第 {n} 个字：{msg}", "Formula, character {n}: {msg}"),
    ("看不懂的数「{text}」", "Can't parse number \"{text}\""),
    (
        "引号没有配对：这段文字少了结束的引号",
        "Unmatched quote: this text is missing its closing quote",
    ),
    ("这里看不懂：{near}", "Can't understand this: {near}"),
    (
        "公式没写完：这里少了「{op}」",
        "Incomplete formula: missing \"{op}\" here",
    ),
    (
        "这里应该是「{op}」，不是「{t}」",
        "Expected \"{op}\" here, not \"{t}\"",
    ),
    ("结尾", "the end"),
    ("公式嵌套太深", "Formula is nested too deeply"),
    ("公式没写完", "Incomplete formula"),
    (
        "「.」后面应该是函数名，比如 .length()",
        "Expected a function name after \".\", e.g. .length()",
    ),
    (
        "「.{name}」后面要跟括号：.{f}()",
        "\".{name}\" must be followed by parentheses: .{f}()",
    ),
    (
        "{f} 的变量名要写成名字，比如 let(x, 1, x + 1)",
        "{f}: the variable must be a name, e.g. let(x, 1, x + 1)",
    ),
    (
        "{f} 的变量名后面要跟「,」和值",
        "{f}: the variable name must be followed by \",\" and a value",
    ),
    (
        "{f} 的参数个数不对（{usage}）",
        "{f}: wrong number of arguments ({usage})",
    ),
    (
        "「{name}」不是属性也不是变量：取属性的值写 prop(\"{name}\")",
        "\"{name}\" is neither a property nor a variable: to get a property's value write prop(\"{name}\")",
    ),
    (
        "prop 里写属性名，比如 prop(\"数量\")",
        "prop takes a property name, e.g. prop(\"Quantity\")",
    ),
    (
        "prop 里只写一个属性名，比如 prop(\"数量\")",
        "prop takes exactly one property name, e.g. prop(\"Quantity\")",
    ),
    ("这里多了「{t}」", "Unexpected \"{t}\" here"),
    ("没有函数「{name}」", "No function \"{name}\""),
    (
        "没有函数「{name}」，是不是 {n}？",
        "No function \"{name}\"; did you mean {n}?",
    ),
    ("公式是空的", "Formula is empty"),
    (
        "公式太长（最多 {n} 字）",
        "Formula is too long (max {n} characters)",
    ),
    ("公式多了「{t}」", "Unexpected \"{t}\" in formula"),
    // 函数的用法（参数个数不对时带上）
    ("if(条件, 是的话, 不是的话)", "if(condition, then, else)"),
    (
        "ifs(条件1, 值1, 条件2, 值2, …, 都不是的话)",
        "ifs(condition1, value1, condition2, value2, …, else)",
    ),
    ("and(甲, 乙, …)", "and(a, b, …)"),
    ("or(甲, 乙, …)", "or(a, b, …)"),
    ("not(甲)", "not(a)"),
    ("empty(值)", "empty(value)"),
    ("let(名字, 值, 表达式)", "let(name, value, expression)"),
    (
        "lets(名字1, 值1, 名字2, 值2, …, 表达式)",
        "lets(name1, value1, name2, value2, …, expression)",
    ),
    ("length(文字或列表)", "length(text or list)"),
    ("concat(甲, 乙, …)", "concat(a, b, …)"),
    ("join(列表, 分隔)", "join(list, separator)"),
    (
        "contains(文字或列表, 找什么)",
        "contains(text or list, search)",
    ),
    ("replace(文字, 旧, 新)", "replace(text, old, new)"),
    ("lower(文字)", "lower(text)"),
    ("upper(文字)", "upper(text)"),
    ("trim(文字)", "trim(text)"),
    (
        "slice(文字或列表, 从, 到)",
        "slice(text or list, start, end)",
    ),
    ("split(文字, 分隔)", "split(text, separator)"),
    ("repeat(文字, 次数)", "repeat(text, times)"),
    ("startsWith(文字, 开头)", "startsWith(text, prefix)"),
    ("endsWith(文字, 结尾)", "endsWith(text, suffix)"),
    (
        "padStart(文字, 长度, 补什么)",
        "padStart(text, length, pad)",
    ),
    ("padEnd(文字, 长度, 补什么)", "padEnd(text, length, pad)"),
    ("toNumber(值)", "toNumber(value)"),
    ("toText(值)", "toText(value)"),
    ("round(数, 小数位)", "round(number, places)"),
    ("floor(数)", "floor(number)"),
    ("ceil(数)", "ceil(number)"),
    ("abs(数)", "abs(number)"),
    ("sqrt(数)", "sqrt(number)"),
    ("cbrt(数)", "cbrt(number)"),
    ("pow(底, 次方)", "pow(base, exponent)"),
    ("mod(被除数, 除数)", "mod(dividend, divisor)"),
    ("sign(数)", "sign(number)"),
    ("exp(数)", "exp(number)"),
    ("ln(数)", "ln(number)"),
    ("log10(数)", "log10(number)"),
    ("log2(数)", "log2(number)"),
    ("min(甲, 乙, …)", "min(a, b, …)"),
    ("max(甲, 乙, …)", "max(a, b, …)"),
    ("sum(甲, 乙, …)", "sum(a, b, …)"),
    ("average(甲, 乙, …)", "average(a, b, …)"),
    ("median(甲, 乙, …)", "median(a, b, …)"),
    (
        "dateAdd(日期, 数, \"days\")",
        "dateAdd(date, number, \"days\")",
    ),
    (
        "dateSubtract(日期, 数, \"days\")",
        "dateSubtract(date, number, \"days\")",
    ),
    (
        "dateBetween(日期1, 日期2, \"days\")：日期1 减 日期2",
        "dateBetween(date1, date2, \"days\"): date1 minus date2",
    ),
    ("dateRange(开始, 结束)", "dateRange(start, end)"),
    ("dateStart(日期)", "dateStart(date)"),
    ("dateEnd(日期)", "dateEnd(date)"),
    (
        "formatDate(日期, \"YYYY-MM-DD\")",
        "formatDate(date, \"YYYY-MM-DD\")",
    ),
    ("year(日期)", "year(date)"),
    ("month(日期)", "month(date)"),
    ("day(日期)：几号", "day(date): day of the month"),
    (
        "weekday(日期)：周一是 1，周日是 7",
        "weekday(date): Monday is 1, Sunday is 7",
    ),
    (
        "week(日期)：一年里第几周（ISO）",
        "week(date): week of the year (ISO)",
    ),
    ("hour(日期)", "hour(date)"),
    ("minute(日期)", "minute(date)"),
    (
        "timestamp(日期)：毫秒时间戳",
        "timestamp(date): timestamp in milliseconds",
    ),
    ("fromTimestamp(毫秒时间戳)", "fromTimestamp(milliseconds)"),
    ("first(列表)", "first(list)"),
    ("last(列表)", "last(list)"),
    (
        "at(列表, 第几个)：从 0 起，负数从后数",
        "at(list, index): starts at 0, negative counts from the end",
    ),
    ("reverse(列表)", "reverse(list)"),
    ("sort(列表)", "sort(list)"),
    ("unique(列表)", "unique(list)"),
    ("flat(列表)", "flat(list)"),
    ("map(列表, current * 2)", "map(list, current * 2)"),
    ("filter(列表, current > 1)", "filter(list, current > 1)"),
    ("find(列表, current > 1)", "find(list, current > 1)"),
    (
        "findIndex(列表, current > 1)",
        "findIndex(list, current > 1)",
    ),
    ("some(列表, current > 1)", "some(list, current > 1)"),
    ("every(列表, current > 1)", "every(list, current > 1)"),
    // 写格子
    ("「{name}」写选项的名字", "\"{name}\": write an option name"),
    ("（还没有选项）", "(no options yet)"),
    (
        "「{name}」没有选项「{s}」，可选：{names}",
        "\"{name}\" has no option \"{s}\"; choose from: {names}",
    ),
    (
        "「{name}」的选项名不能超过 {n} 字",
        "Option names in \"{name}\" can't exceed {n} characters",
    ),
    (
        "「{name}」最多 {n} 个选项",
        "\"{name}\" can have at most {n} options",
    ),
    (
        "「{name}」是自动算的，不能改",
        "\"{name}\" is computed automatically and can't be changed",
    ),
    (
        "「{name}」是按钮，没有值",
        "\"{name}\" is a button and has no value",
    ),
    ("「{name}」要写一段文字", "\"{name}\" needs text"),
    (
        "「{name}」太长（最多 {n} 字）",
        "\"{name}\" is too long (max {n} characters)",
    ),
    (
        "「{name}」不像邮箱：{s}",
        "\"{name}\" doesn't look like an email: {s}",
    ),
    (
        "「{name}」要写数字，不是「{v}」",
        "\"{name}\" needs a number, not \"{v}\"",
    ),
    ("「{name}」只能选一个", "\"{name}\" allows only one choice"),
    (
        "「{name}」的日期写成 2026-11-15、2026-11-15 14:00、2026年11月15日 或 今天 / 明天 / 昨天，不是「{v}」",
        "\"{name}\": write dates like 2026-11-15, 2026-11-15 14:00 or today / tomorrow / yesterday, not \"{v}\"",
    ),
    (
        "「{name}」的结束日期写成 2026-11-15、2026-11-15 14:00、2026年11月15日 或 今天 / 明天 / 昨天，不是「{v}」",
        "\"{name}\": write the end date like 2026-11-15, 2026-11-15 14:00 or today / tomorrow / yesterday, not \"{v}\"",
    ),
    (
        "「{name}」的结束日期在开始日期前面",
        "\"{name}\": the end date is before the start date",
    ),
    (
        "「{name}」没有叫「{s}」的人",
        "\"{name}\": no person named \"{s}\"",
    ),
    (
        "「{name}」要写 是 / 否（true / false）",
        "\"{name}\" needs yes / no (true / false)",
    ),
    ("不认识的类型：{k}", "Unknown type: {k}"),
    (
        "「{name}」写行 id 或者那一行的名称",
        "\"{name}\": write a row id or the row's name",
    ),
    (
        "「{name}」关联的库里没有叫「{s}」的行",
        "\"{name}\": the related database has no row named \"{s}\"",
    ),
    (
        "「{name}」最多关联 {n} 行",
        "\"{name}\" can link at most {n} rows",
    ),
    (
        "「{name}」要写网址（https://…）或者传上来的文件，不是「{url}」",
        "\"{name}\" needs a URL (https://…) or an uploaded file, not \"{url}\"",
    ),
    (
        "「{name}」最多 {n} 个文件",
        "\"{name}\" can have at most {n} files",
    ),
    // 筛选、排序、分组
    ("筛选嵌套太深", "Filter is nested too deeply"),
    (
        "筛选要指明属性（prop）",
        "Each filter needs a property (prop)",
    ),
    (
        "筛选的格式不对：{ op: \"and\", items: [{ prop, cond, value }] }",
        "Bad filter format: { op: \"and\", items: [{ prop, cond, value }] }",
    ),
    (
        "筛选里没有属性「{prop}」",
        "Filter refers to unknown property \"{prop}\"",
    ),
    (
        "「{name}」（{kind}）不能用条件「{cond}」，能用：{list}",
        "\"{name}\" ({kind}) doesn't support condition \"{cond}\"; available: {list}",
    ),
    (
        "「{name}」写编号，比如 {prefix}12",
        "\"{name}\": write an ID, e.g. {prefix}12",
    ),
    (
        "「{name}」的范围写 today、this_week、this_month、this_year、past_week、past_month、past_year、next_week、next_month、next_year 或 past_N_days / next_N_days，不是「{v}」",
        "\"{name}\": ranges are today, this_week, this_month, this_year, past_week, past_month, past_year, next_week, next_month, next_year or past_N_days / next_N_days, not \"{v}\"",
    ),
    (
        "「{name}」的日期写成 2026-11-15 或 today / 明天 / one_week_ago…，不是「{v}」",
        "\"{name}\": write dates like 2026-11-15 or today / tomorrow / one_week_ago…, not \"{v}\"",
    ),
    ("「{name}」不能筛选", "\"{name}\" can't be filtered"),
    (
        "排序里没有属性「{p}」",
        "Sort refers to unknown property \"{p}\"",
    ),
    (
        "分组里没有属性「{k}」",
        "Group refers to unknown property \"{k}\"",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    static T: Table = &[
        ("页面 {id} 不存在", "Page {id} not found"),
        ("第 {n} 步：{why}", "Step {n}: {why}"),
        ("找不到块 {id}", "Block {id} not found"),
        ("没有变化", "No changes"),
        (
            "「{name}」要写数字，不是「{v}」",
            "\"{name}\" needs a number, not \"{v}\"",
        ),
    ];

    #[test]
    fn forward_and_back() {
        assert_eq!(
            t_in(&[T], Lang::En, "页面 {id} 不存在", &[("id", "abc")]),
            "Page abc not found"
        );
        assert_eq!(
            t_in(&[T], Lang::Zh, "页面 {id} 不存在", &[("id", "abc")]),
            "页面 abc 不存在"
        );
        assert_eq!(t_in(&[T], Lang::En, "表里没有的话", &[]), "表里没有的话");
        assert_eq!(translate(&[T], Lang::En, "没有变化"), "No changes");
        assert_eq!(
            translate(&[T], Lang::En, "页面 x1 不存在"),
            "Page x1 not found"
        );
        assert_eq!(
            translate(&[T], Lang::En, "第 3 步：找不到块 b9"),
            "Step 3: Block b9 not found"
        );
        assert_eq!(
            translate(&[T], Lang::En, "「价格」要写数字，不是「很贵」"),
            "\"价格\" needs a number, not \"很贵\""
        );
        assert_eq!(translate(&[T], Lang::En, "对不上的"), "对不上的");
        assert_eq!(fill("{a}{b} {c}", &[("a", "1"), ("b", "2")]), "12 {c}");
        assert_eq!(fill("JSON {\"x\": 1}", &[]), "JSON {\"x\": 1}");
    }

    #[test]
    fn accept_language() {
        assert_eq!(
            Lang::from_accept("en-US,en;q=0.9,zh-CN;q=0.8"),
            Some(Lang::En)
        );
        assert_eq!(Lang::from_accept("zh-CN,zh;q=0.9,en;q=0.8"), Some(Lang::Zh));
        assert_eq!(Lang::from_accept("fr-FR,en;q=0.5"), Some(Lang::En));
        assert_eq!(Lang::from_accept("fr"), None);
        assert_eq!(
            check_locale(Some("en-GB".into())).unwrap().as_deref(),
            Some("en")
        );
        assert!(check_locale(Some("fr".into())).is_err());
    }
}
