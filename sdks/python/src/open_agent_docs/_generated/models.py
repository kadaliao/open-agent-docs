# 由 codegen 生成，不要手改：python sdks/python/codegen/generate.py（来源 docs/openapi.json、sdks/operations.json）
"""接口的模型（docs/openapi.json 的 components.schemas 和 webhooks）。"""
from __future__ import annotations

import warnings
from typing import Any, Dict, List, Literal, Optional, Type, Union

from pydantic import Field

from .._models import BaseModel

__all__ = [
    "Ack",
    "Acked",
    "AddRow",
    "AddRows",
    "AddedRows",
    "Agent",
    "AgentList",
    "AgentNotes",
    "AgentNotification",
    "AgentView",
    "Anchor",
    "AuthConfigBody",
    "AuthMode",
    "AuthorRef",
    "Authors",
    "BlockBody",
    "BlockContent",
    "BlockDiff",
    "BlockIds",
    "CellChange",
    "CellOp",
    "CommentCreatedEvent",
    "CommentView",
    "Content",
    "CreateAgent",
    "CreatePage",
    "CreatedAgent",
    "DbChangedEvent",
    "DbHistory",
    "DbHistoryItem",
    "DbInfo",
    "DbProp",
    "DbRow",
    "DbSchema",
    "DbSchemaEvent",
    "DbSort",
    "DbUndone",
    "DbView",
    "Deleted",
    "DiffTo",
    "DocMark",
    "DocNode",
    "EditComment",
    "ErrorBody",
    "ErrorCode",
    "ErrorInfo",
    "Ids",
    "ImportForm",
    "Inbox",
    "InboxNotification",
    "InviteBy",
    "InviteInfo",
    "InviteList",
    "InviteView",
    "InviteWorkspace",
    "LoginBody",
    "Me",
    "MePatch",
    "MeUser",
    "MeWorkspace",
    "Member",
    "MemberList",
    "MemberRole",
    "MemberUser",
    "NewDb",
    "NewInvite",
    "NewSecret",
    "NewThread",
    "NewToken",
    "NoteAgent",
    "NoteComment",
    "NotePage",
    "Notify",
    "Page",
    "PageList",
    "PagePurgedEvent",
    "PasswordBody",
    "PatchAgent",
    "PatchPage",
    "PatchRows",
    "PatchThread",
    "Patched",
    "Pressed",
    "PrincipalKind",
    "PropType",
    "PutContent",
    "PutShare",
    "PutViews",
    "QueryGroup",
    "QueryReq",
    "QueryResult",
    "QueryRow",
    "ReadCount",
    "ReadNotes",
    "RelationConf",
    "Reply",
    "Restored",
    "RestoredPage",
    "Role",
    "RoleBody",
    "RollupConf",
    "RowsDelta",
    "SaveBody",
    "SchemaChanged",
    "SchemaOps",
    "SchemaUndone",
    "Scope",
    "SearchResult",
    "SearchResults",
    "SelectOption",
    "Seq",
    "ShareView",
    "SignupBody",
    "SignupMode",
    "ThreadList",
    "ThreadView",
    "TrashItem",
    "TrashList",
    "Undo",
    "UndoBody",
    "UnfurlCard",
    "UploadForm",
    "Uploaded",
    "VersionAuthor",
    "VersionContent",
    "VersionDiff",
    "VersionList",
    "VersionMeta",
    "ViewObj",
    "ViewType",
    "Views",
    "Workspace",
    "WsPatch",
]

# 有的字段叫 schema、json，和 pydantic.BaseModel 的老方法同名：照样能用（拿到的是字段），警告压掉
warnings.filterwarnings("ignore", message=r"Field name \"(construct|copy|dict|fields|from_orm|json|parse_obj|parse_raw|schema|validate)\" .* shadows an attribute")


class Ack(BaseModel):
    """处理完了：给 ids，或者 upto（编号 ≤ upto 的都算），至少给一个。"""
    ids: Optional[List[int]] = None
    """这些编号"""
    upto: Optional[int] = None
    """编号 ≤ 这个的"""


class Acked(BaseModel):
    acked: int
    """标了几条"""


class AddRow(BaseModel):
    after: Optional[str] = None
    """排在这一行后面（不给放最后）"""
    body: Optional[str] = None
    """行页面的正文（Markdown）"""
    props: Optional[Dict[str, Any]] = None
    """属性：键是属性的 key 或名字，值是存的样子（选项也可以写名字）"""


class AddRows(BaseModel):
    rows: List[AddRow]


class AddedRows(BaseModel):
    ids: List[str]
    """新行的 id（和给的顺序一样）"""
    seq: int


class Agent(BaseModel):
    about: str
    created_at: int
    created_by: Optional[str] = None
    id: str
    last_used_at: Optional[int] = None
    name: str
    revoked_at: Optional[int] = None
    scope: Scope
    webhook: Optional[str] = None
    workspace_id: str


class AgentList(BaseModel):
    agents: List[AgentView]


class AgentNotes(BaseModel):
    next_cursor: Optional[str] = None
    """取满了一页时是最后一条的编号，带 `?cursor=` 接着取；没有就是取完了"""
    notifications: List[AgentNotification]


class AgentNotification(BaseModel):
    """Agent 收到的一条通知（webhook 的正文；长轮询 / SSE 取到的也是这样）。"""
    agent: NoteAgent
    block_id: Optional[str] = None
    """评论挂在哪一块上（对整页的评论是 null）"""
    comment: NoteComment
    created_at: int
    id: int
    """通知编号（处理完带回来当 after / cursor）"""
    page: NotePage
    prompt: str
    """一段可以直接当消息发给 Agent 的话（按 Agent 的语言）"""
    quote: Optional[str] = None
    """选中的原文"""
    thread_id: str
    type: str
    """comment"""


class AgentView(Agent):
    """Agent 列表里的一项：加上没处理的通知数和最近的 webhook 错误。"""
    pending: int
    webhook_error: Optional[str] = None


class Anchor(BaseModel):
    """评论挂在哪：一段文字（start / end 是网页给的 Yjs RelativePosition，服务器原样存）、整段（只有 block_id）或整页（None）。"""
    block_id: Optional[str] = None
    end: Optional[str] = None
    quote: Optional[str] = None
    start: Optional[str] = None


class AuthConfigBody(BaseModel):
    """登录方式和站点设置。"""
    allowed_origins: Optional[List[str]] = None
    """允许跨源嵌入的宿主（`OAD_ALLOWED_ORIGINS`，加上 `OAD_PUBLIC_URL` 的 origin），如 `https://app.example.com`。
    iframe 里的网页只认这些 origin（和同源）发来的 `oad:token`。旧版服务器没有这个字段
    """
    mode: AuthMode
    """登录方式：local（账号密码）/ header（宿主反代带身份头）/ none（不登录，单人）/ jwt（宿主签的 JWT）"""
    signup: SignupMode
    timezone: str
    """服务器的时区（IANA 名字）"""


class AuthorRef(BaseModel):
    id: str
    kind: PrincipalKind
    name: str


class Authors(BaseModel):
    """每个 Yjs client id（字符串）是谁写的。"""
    clients: Dict[str, AuthorRef]


class BlockBody(BaseModel):
    after: Optional[str] = None
    base: Optional[str] = None
    markdown: str


class BlockContent(BaseModel):
    block: DocNode
    markdown: str
    version: str


class BlockDiff(BaseModel):
    """对比里的一块。"""
    after: Optional[str] = None
    before: Optional[str] = None
    by: Optional[List[VersionAuthor]] = None
    """这块里在两版之间写了字的人"""
    id: str
    op: str
    """added / removed / changed / same"""
    type: str
    """块的类型（paragraph、heading…）"""


class BlockIds(BaseModel):
    ids: List[str]


class CellChange(BaseModel):
    """一格的改动。"""
    new: Any = None
    old: Any = None
    prop: str
    row_id: str


class CellOp(BaseModel):
    """改一行的几格：值为 null 的清掉。给了 `after` 是挪位置（排在那一行后面，null 是最前面）。"""
    after: Optional[str] = None
    row_id: str
    set: Optional[Dict[str, Any]] = None


class CommentCreatedEvent(BaseModel):
    """SSE 事件 `comment.created` 的 data。"""
    comment_id: Optional[str] = None
    page_id: Optional[str] = None
    thread: Optional[ThreadView] = None
    thread_id: Optional[str] = None


class CommentView(BaseModel):
    author: AuthorRef
    body: str
    """删掉的是空字符串。"""
    created_at: int
    deleted_at: Optional[int] = None
    edited_at: Optional[int] = None
    id: str
    thread_id: str


class Content(BaseModel):
    """正文：`markdown`（默认）、`doc`（`?format=json`）或 `html`（`?format=html`，只读），带当前版本（改的时候作为 base 带回来）。"""
    doc: Optional[DocNode] = None
    html: Optional[str] = None
    """只读 HTML（`?format=html`）：外面包 `<div class="oad-content">`，配 `/embed/content.css` 显示。
    只有白名单里的标签和属性；公式、链接卡片是带 `data-*` 的占位；设了 `OAD_PUBLIC_URL` 时文件、页面链接是完整地址
    """
    markdown: Optional[str] = None
    total: Optional[int] = None
    """顶层块一共几个（给了 `limit` 才有）"""
    version: str


class CreateAgent(BaseModel):
    about: Optional[str] = None
    """介绍（给人和别的 Agent 看）"""
    name: str
    """1 到 40 个字，不能有空白和 @ [ ] ( ) < > 引号 逗号"""
    scope: str
    """read / write / admin"""
    webhook: Optional[str] = None
    """收通知的 webhook 地址"""


class CreatePage(BaseModel):
    after: Optional[str] = None
    icon: Optional[str] = None
    parent_id: Optional[str] = None
    title: Optional[str] = None


class CreatedAgent(BaseModel):
    """新建的 Agent；token 和 webhook_secret 只有这一次能看到。"""
    agent: Agent
    token: str
    """`oad_…`，当 Bearer token 用"""
    webhook_secret: str
    """`whsec_…`，验 webhook 的签名"""


class DbChangedEvent(BaseModel):
    """SSE 事件 `db.changed` 的 data。"""
    db_id: Optional[str] = None
    seq: Optional[int] = None


class DbHistory(BaseModel):
    """库的历史。"""
    items: List[DbHistoryItem]
    """新的在前；同一次改动（同一个 seq）的格子不会分在两页"""
    next_cursor: Optional[str] = None
    """还有下一页时给"""
    seq: int
    """库现在的 seq"""


class DbHistoryItem(BaseModel):
    """历史里的一条。`kind`：`cell`（一格改了）、`row`（加 / 删了一行）、`schema`（表结构改了，只在不按行 / 属性筛时有）。"""
    at: int
    by: Optional[Union[AuthorRef, str]] = None
    cells: Optional[int] = None
    """schema 有：顺带改了几格"""
    changes: Optional[List[Dict[str, Any]]] = None
    """schema 有：这次动了哪些属性"""
    kind: str
    """cell / row / schema"""
    new: Any = None
    old: Any = None
    prop: Optional[str] = None
    """cell 有：属性的 key"""
    prop_name: Optional[str] = None
    """cell 有：属性的名字"""
    row_id: Optional[str] = None
    """cell / row 有"""
    row_title: Optional[str] = None
    """cell / row 有"""
    seq: int
    summary: Optional[str] = None
    """row / schema 有：一句话说明"""
    undone: Optional[bool] = None
    """schema 有：撤销过了"""


class DbInfo(BaseModel):
    """库的信息（GET /databases/{id}）。"""
    page: Page
    schema: DbSchema  # pyright: ignore[reportIncompatibleMethodOverride, reportIncompatibleVariableOverride]
    seq: int
    views: List[DbView]


class DbProp(BaseModel):
    """一个属性（一列）。`key` 是短随机串，改名不动数据；各类型自己的配置只在那个类型上有。"""
    agent: Optional[str] = None
    """button 按下时通知的 Agent（id）"""
    comment: Optional[str] = None
    """button 按下时发的评论"""
    date_format: Optional[str] = None
    """日期的显示：iso | zh | slash | relative"""
    format: Optional[str] = None
    """数字的格式：number | comma | percent | cny | usd（number、formula、rollup）"""
    formula: Optional[str] = None
    """formula 的公式"""
    key: str
    label: Optional[str] = None
    """button 的文字"""
    name: str
    options: Optional[List[SelectOption]] = None
    """select / multi_select / status 的选项"""
    precision: Optional[int] = None
    """显示几位小数"""
    prefix: Optional[str] = None
    """unique_id 的前缀（T-12）"""
    relation: Optional[RelationConf] = None
    rollup: Optional[RollupConf] = None
    set: Optional[Dict[str, Any]] = None
    """button 按下时设的值：`{ 属性 key 或名字: 值 }`"""
    type: PropType


class DbRow(BaseModel):
    """库里的一行。`props` 是存的样子（键是属性的 key），标题也在里面（和行页面的标题同步）。"""
    created_at: int
    created_by: Optional[str] = None
    db_id: str
    deleted_at: Optional[int] = None
    id: str
    position: str
    props: Dict[str, Any]
    seq: int
    unique_id: int
    updated_at: int
    updated_by: Optional[str] = None


class DbSchema(BaseModel):
    """表结构。"""
    next_id: Optional[int] = None
    """下一个自动编号"""
    props: List[DbProp]


class DbSchemaEvent(BaseModel):
    """SSE 事件 `db.schema` 的 data。"""
    db_id: Optional[str] = None


class DbSort(BaseModel):
    """排序：`dir: "asc" | "desc"`。"""
    dir: Optional[str] = None
    prop: str


class DbUndone(BaseModel):
    """撤销的结果：格子改动只有 `seq`；表结构改动还有新的 `schema`、`views`。"""
    schema: Optional[DbSchema] = None  # pyright: ignore[reportIncompatibleMethodOverride, reportIncompatibleVariableOverride]
    seq: int
    """撤销后库的 seq"""
    views: Optional[List[DbView]] = None


class DbView(BaseModel):
    """一个视图。存的时候按表结构清理（去掉指向已删属性的排序、分组、隐藏、列宽）。"""
    calendar_by: Optional[str] = None
    """日历按哪个日期属性"""
    filter: Optional[Dict[str, Any]] = None
    """筛选：`{ op: "and" | "or", items: [{ prop, op, value } 或嵌套的组] }`"""
    group_by: Optional[str] = None
    """按哪个属性分组（看板的列）"""
    group_date: Optional[str] = None
    """按日期分组时怎么分：day | week | month | year | relative"""
    hidden: Optional[List[str]] = None
    """隐藏的属性 key"""
    hide_empty_groups: Optional[bool] = None
    """去掉没有行的分组"""
    id: str
    name: str
    order: Optional[List[str]] = None
    """列的顺序（属性 key）"""
    sorts: Optional[List[DbSort]] = None
    sub_group_by: Optional[str] = None
    sub_group_date: Optional[str] = None
    totals: Optional[Dict[str, Any]] = None
    """每列底部的汇总：`{ key: fn }`"""
    type: ViewType
    widths: Optional[Dict[str, Any]] = None
    """列宽：`{ key: 像素 }`"""


class Deleted(BaseModel):
    deleted: List[str]
    """进了回收站的页面（连子页面）"""


class DocMark(BaseModel):
    attrs: Optional[Dict[str, Any]] = None
    """没有属性时不给"""
    type: str


class DocNode(BaseModel):
    """正文的节点树（和编辑器的 ProseMirror JSON 一样）：`{ type, attrs?, content?, text?, marks? }`。"""
    attrs: Optional[Dict[str, Any]] = None
    """没有属性时不给"""
    content: Optional[List[DocNode]] = None
    marks: Optional[List[DocMark]] = None
    text: Optional[str] = None
    type: str


class EditComment(BaseModel):
    body: str
    """新的正文（Markdown）"""


class ErrorBody(BaseModel):
    """统一的错误：`{ error: { code, message } }`。message 按调用者的语言（中文 / 英文）。"""
    error: ErrorInfo


class ErrorInfo(BaseModel):
    code: ErrorCode
    message: str
    """给人看的原因"""


class Ids(BaseModel):
    ids: List[str]
    """要删的行（进回收站）"""


class ImportForm(BaseModel):
    """multipart 导入的字段。"""
    file: str
    """CSV 文件（第一行是列名）"""
    parent_id: Optional[str] = None
    """放在哪个页面下"""
    title: Optional[str] = None
    """库名"""


class Inbox(BaseModel):
    next_cursor: Optional[str] = None
    """取满了一页时是最后一条（最旧的）的编号，带 `?cursor=` 往旧的翻；没有就是到头了"""
    notifications: List[InboxNotification]
    """新的在前"""
    unread: int
    """没读的总数"""


class InboxNotification(BaseModel):
    """收件箱里的一条。"""
    created_at: int
    data: Dict[str, Any]
    """页面、评论串、谁写的之类（按 type 不同）"""
    id: int
    read_at: Optional[int] = None
    """没读是 null"""
    type: str
    """mention / reply / resolved"""
    workspace_id: str


class InviteBy(BaseModel):
    name: str


class InviteInfo(BaseModel):
    """邀请链接打开时看到的（不用登录）。"""
    expires_at: Optional[int] = None
    invited_by: Optional[InviteBy] = None
    role: Role
    signup: bool
    """能不能凭它注册"""
    workspace: InviteWorkspace


class InviteList(BaseModel):
    invites: List[InviteView]
    """还能用的"""


class InviteView(BaseModel):
    """一条邀请链接。"""
    created_at: int
    expires_at: Optional[int] = None
    """毫秒时间戳；null = 不过期"""
    role: Role
    """凭它加入后的角色"""
    token: str
    url: str
    """发给别人的完整链接"""
    uses: int
    """用过几次"""


class InviteWorkspace(BaseModel):
    """邀请到哪个工作区。"""
    icon: Optional[str] = None
    id: str
    name: str


class LoginBody(BaseModel):
    email: str
    password: str


class Me(BaseModel):
    """我是谁、在哪些工作区。"""
    user: MeUser
    workspaces: List[MeWorkspace]


class MePatch(BaseModel):
    """改个人资料：只改给了的字段；给 null 是清掉。"""
    avatar: Optional[str] = None
    """头像（emoji 或图片地址）"""
    locale: Optional[str] = None
    """zh / en；null = 跟浏览器"""
    name: Optional[str] = None
    timezone: Optional[str] = None
    """IANA 时区名；null = 跟服务器"""


class MeUser(BaseModel):
    avatar: Optional[str] = None
    email: Optional[str] = None
    id: str
    kind: PrincipalKind
    locale: Optional[str] = None
    """zh / en / null（跟浏览器）"""
    name: str
    timezone: Optional[str] = None


class MeWorkspace(BaseModel):
    icon: Optional[str] = None
    id: str
    name: str
    role: Role


class Member(BaseModel):
    role: Role
    user: MemberUser


class MemberList(BaseModel):
    """工作区的人（按加入先后）和 Agent。"""
    agents: List[AgentView]
    members: List[Member]


class MemberRole(BaseModel):
    role: Role
    user_id: str


class MemberUser(BaseModel):
    avatar: Optional[str] = None
    email: Optional[str] = None
    id: str
    name: str


class NewDb(BaseModel):
    """建库（POST /workspaces/{ws}/databases 的正文）。"""
    after: Optional[str] = None
    """放在同级里这个页面后面"""
    parent_id: Optional[str] = None
    schema: Optional[Dict[str, Any]] = None  # pyright: ignore[reportIncompatibleMethodOverride, reportIncompatibleVariableOverride]
    """表结构 `{ props: [{ name, type, … }] }`，和 DbSchema 一样，但属性可以不给 key（自动分配），
    选项可以只写名字（`options: ["做", "完"]`）。不给只有标题
    """
    title: Optional[str] = None
    views: Optional[List[Dict[str, Any]]] = None
    """视图（和 DbView 一样，id 可以不给）；不给是一个表格视图"""


class NewInvite(BaseModel):
    expires_in: Optional[int] = None
    """多少秒后过期；不给 = 不过期"""
    role: str
    """viewer / editor / admin / owner"""


class NewSecret(BaseModel):
    webhook_secret: str
    """新的 webhook 签名密钥"""


class NewThread(BaseModel):
    anchor: Optional[Anchor] = None
    body: str
    """Markdown；`@[名字](agent:<id>)` / `@[名字](user:<id>)` 或 `@名字` 叫人"""
    notify: Optional[Notify] = None


class NewToken(BaseModel):
    token: str
    """新 token（旧的马上失效）"""


class NoteAgent(BaseModel):
    id: str
    name: str


class NoteComment(BaseModel):
    author: Dict[str, Any]
    """`{ kind, id, name }`"""
    body: str
    id: str


class NotePage(BaseModel):
    id: str
    title: str
    url: str


class Page(BaseModel):
    created_at: int
    created_by: Optional[str] = None
    deleted_at: Optional[int] = None
    icon: Optional[str] = None
    id: str
    kind: Optional[str] = None
    """page / database / row。"""
    parent_id: Optional[str] = None
    position: str
    title: str
    updated_at: int
    workspace_id: str


class PageList(BaseModel):
    pages: List[Page]
    """不含回收站里的、库里的行；按 (parent_id, position) 排"""


class PagePurgedEvent(BaseModel):
    """SSE 事件 `page.purged` 的 data。"""
    id: Optional[str] = None


class PasswordBody(BaseModel):
    current: str
    """现在的密码"""
    new: str
    """新密码"""


class PatchAgent(BaseModel):
    about: Optional[str] = None
    locale: Optional[str] = None
    """通知里那段话用的语言：zh / en；null = 跟工作区"""
    name: Optional[str] = None
    scope: Optional[str] = None
    """read / write / admin"""
    webhook: Optional[str] = None
    """null = 不用 webhook"""


class PatchPage(BaseModel):
    after: Optional[str] = None
    icon: Optional[str] = None
    parent_id: Optional[str] = None
    title: Optional[str] = None


class PatchRows(BaseModel):
    ops: List[CellOp]


class PatchThread(BaseModel):
    resolved: bool
    """true = 解决，false = 重新打开"""


class Patched(BaseModel):
    """改了的格子和改完的 seq。"""
    changes: List[CellChange]
    """真的变了的格子（值没变的不在里面）"""
    seq: int


class Pressed(BaseModel):
    """按按钮的结果。"""
    changes: List[CellChange]
    """按钮设的属性里真的变了的格子"""
    seq: int
    thread_id: Optional[str] = None
    """按钮带评论时，发在这一行上的评论串"""


class PutContent(BaseModel):
    base: Optional[str] = None
    """读的时候拿到的版本：在那个版本上改，这期间别人写的不会被覆盖"""
    doc: Optional[DocNode] = None
    markdown: Optional[str] = None


class PutShare(BaseModel):
    """分享设置；不给正文就都是 false。"""
    include_children: Optional[bool] = None
    """子页面也公开"""
    indexable: Optional[bool] = None
    """允许搜索引擎收录"""


class QueryGroup(BaseModel):
    """一个分组。空值的分组 `key` 是 ""，排在最后。"""
    color: Optional[str] = None
    count: int
    groups: Optional[List[QueryGroup]] = None
    """子分组（视图有 sub_group_by 时）"""
    ids: List[str]
    key: str
    name: str
    totals: Optional[Dict[str, Any]] = None


class QueryReq(BaseModel):
    """服务器算的查询（给 Agent、给不想加载全部数据的客户端）。给了 view_id 就从那个视图出发，别的字段覆盖它。"""
    filter: Optional[Dict[str, Any]] = None
    """筛选 `{ op: "and" | "or", items: [...] }`"""
    group_by: Optional[str] = None
    limit: Optional[int] = None
    offset: Optional[int] = None
    search: Optional[str] = None
    sorts: Optional[List[Dict[str, Any]]] = None
    """排序 `[{ key, dir }]`"""
    totals: Optional[Dict[str, Any]] = None
    """每列底部的汇总：`{ 属性 key: 汇总方式 }`"""
    view_id: Optional[str] = None


class QueryResult(BaseModel):
    """查询结果。"""
    groups: Optional[List[QueryGroup]] = None
    """按 group_by 分的组；没分组不给"""
    ids: List[str]
    """按排序的行 id（给了 limit / offset 是那一段）"""
    rows: List[QueryRow]
    """这一段的行，带算出来的值"""
    seq: int
    """库现在的 seq"""
    total: int
    """符合条件的总行数"""
    totals: Optional[Dict[str, Any]] = None
    """每列底部的汇总（键是属性的 key）；没要不给"""


class QueryRow(BaseModel):
    """查出来的一行：存的样子（`props`，键是属性的 key，空值不存），加上 `computed`（公式、汇总、反向关联算出来的值）。
    和 `DbRow` 不同：不带 `db_id`、`position`（顺序看 `ids`），时间是毫秒数。
    """
    computed: Dict[str, Any]
    created_at: Optional[float] = None
    """毫秒时间戳"""
    created_by: Optional[str] = None
    id: str
    props: Dict[str, Any]
    seq: int
    unique_id: Optional[int] = None
    """自动编号"""
    updated_at: Optional[float] = None
    """毫秒时间戳"""
    updated_by: Optional[str] = None


class ReadCount(BaseModel):
    read: int
    """标了几条"""


class ReadNotes(BaseModel):
    """标成已读：给 ids，或者 `all: true`。"""
    all: Optional[bool] = None
    """全部"""
    ids: Optional[List[int]] = None
    """这些编号"""


class RelationConf(BaseModel):
    """关联：指向哪个库；`mirror_of` 有值时是双向关联的反向那一半。"""
    db_id: str
    mirror_of: Optional[str] = None


class Reply(BaseModel):
    body: str
    """Markdown"""
    notify: Optional[Notify] = None


class Restored(BaseModel):
    version: VersionMeta
    """恢复后新存的一版（kind = restore）"""


class RestoredPage(Page):
    """放回来的那个页面，加上 `restored`（连同放回来的所有页面 id）。"""
    restored: List[str]
    """放回来的所有页面（这个和一起删的子页面）"""


class RoleBody(BaseModel):
    role: str
    """viewer / editor / admin / owner"""


class RollupConf(BaseModel):
    """汇总：沿关联属性 `relation` 取对方库的属性 `prop`，按 `fn` 归总。"""
    fn: str
    prop: str
    relation: str


class RowsDelta(BaseModel):
    """增量（GET /databases/{id}/rows）。"""
    deleted: List[str]
    rows: List[DbRow]
    seq: int


class SaveBody(BaseModel):
    label: Optional[str] = None
    """这一版的名字"""


class SchemaChanged(BaseModel):
    """改完的表结构。"""
    changes: List[Dict[str, Any]]
    """动了哪些属性（`{ op, key, name, … }`）"""
    schema: DbSchema  # pyright: ignore[reportIncompatibleMethodOverride, reportIncompatibleVariableOverride]
    """新的表结构"""
    seq: int
    """这次改动的 seq（历史里撤销用；什么也没变时是现在的 seq）"""
    warnings: Optional[List[str]] = None
    """改了但有问题的地方（比如公式有错）；没有就不给"""


class SchemaOps(BaseModel):
    ops: List[Dict[str, Any]]
    """依次做的改动：`{ op: "add" | "update" | "delete" | "move" | …, … }`，见 docs/phase4-contract.md"""


class SchemaUndone(BaseModel):
    """撤销后的表结构和视图。"""
    schema: DbSchema  # pyright: ignore[reportIncompatibleMethodOverride, reportIncompatibleVariableOverride]
    seq: int
    views: List[DbView]


class SearchResult(BaseModel):
    """一条搜索结果（GET /workspaces/{ws}/search）。"""
    block_id: Optional[str] = None
    blocks: int
    """这个页面命中了几处（标题、正文的段）"""
    db_id: Optional[str] = None
    """行所在的库"""
    icon: Optional[str] = None
    kind: str
    """page / database / row"""
    page_id: str
    score: float
    snippet: str
    title: str
    updated_at: int


class SearchResults(BaseModel):
    results: List[SearchResult]


class SelectOption(BaseModel):
    """选项。status 的有 `group`：todo / doing / done。"""
    color: str
    group: Optional[str] = None
    id: str
    name: str


class Seq(BaseModel):
    """改完后库的 seq。"""
    seq: int


class ShareView(BaseModel):
    """页面的公开分享链接。"""
    created_at: int
    include_children: bool
    """子页面也公开"""
    indexable: bool
    """允许搜索引擎收录"""
    token: str
    url: str
    """公开的网址（`/s/<token>`）"""


class SignupBody(BaseModel):
    email: str
    invite: Optional[str] = None
    """邀请的 token（OAD_SIGNUP=invite 时必须有），注册完直接加入那个工作区"""
    name: Optional[str] = None
    """显示的名字；不给就用邮箱 @ 前面那段"""
    password: str


class ThreadList(BaseModel):
    threads: List[ThreadView]


class ThreadView(BaseModel):
    anchor: Optional[Anchor] = None
    comments: List[CommentView]
    created_at: int
    created_by: AuthorRef
    id: str
    page_id: str
    resolved_at: Optional[int] = None
    resolved_by: Optional[AuthorRef] = None


class TrashItem(BaseModel):
    deleted_at: int
    deleted_by: Optional[AuthorRef] = None
    icon: Optional[str] = None
    id: str
    kind: str
    parent_id: Optional[str] = None
    parent_title: Optional[str] = None
    """原来在哪个页面下（行是所在的库）"""
    title: str


class TrashList(BaseModel):
    """回收站里的页面、库、行（只列单独删的，新删的在前）。"""
    items: List[TrashItem]
    next_cursor: Optional[str] = None
    """还有下一页时给"""


class Undo(BaseModel):
    seq: int
    """要撤销的那次表结构改动"""


class UndoBody(BaseModel):
    seq: int
    """要撤销的那次改动（历史里的 seq）"""


class UnfurlCard(BaseModel):
    """链接卡片。除了 kind / provider / source / url 都可能缺；按 kind 还有别的字段
    （canonical_url、description、site_name、favicon、author、thumbnail、media、published_at、embed；
    上传的文件有 name、mime、size、width、height、blurhash、duration_ms；本站的库有 db_id、view_id、icon、views）。
    """
    kind: str
    """video / post / repo / issue / pull_request / article / image / file / bookmark / database"""
    provider: str
    """youtube / x / github / bilibili / wechat / generic；上传的文件是 upload，本站的库是 oad"""
    source: str
    """api / syndication / oembed / html / url_only"""
    stale: Optional[bool] = None
    """过期了先给旧的，后台在刷新"""
    title: Optional[str] = None
    url: str


class UploadForm(BaseModel):
    """multipart 上传的表单。"""
    file: str
    """文件（文件名取这个字段的 filename）"""


class Uploaded(BaseModel):
    """上传接口的返回。"""
    blurhash: Optional[str] = None
    duration_ms: Optional[int] = None
    height: Optional[int] = None
    id: str
    mime: str
    name: str
    size: int
    url: str
    width: Optional[int] = None


class VersionAuthor(BaseModel):
    """一版里改过的人。"""
    id: str
    kind: PrincipalKind
    name: str


class VersionContent(BaseModel):
    """某一版的内容。"""
    doc: DocNode
    markdown: str
    version: VersionMeta


class VersionDiff(BaseModel):
    blocks: List[BlockDiff]
    """按块的增删改，按新的顺序"""
    from_: int = Field(alias="from")
    to: DiffTo


class VersionList(BaseModel):
    """版本列表，新的在前。"""
    next_cursor: Optional[str] = None
    """还有下一页时给，带上它再取"""
    versions: List[VersionMeta]


class VersionMeta(BaseModel):
    """历史版本（不含快照）。kind：auto（停下来 / 改得多了自动存）、agent（Agent 的一次写入）、restore（恢复）、manual（手动存，带名字）。"""
    authors: List[VersionAuthor]
    created_at: int
    created_by: Optional[str] = None
    id: int
    kind: str
    label: Optional[str] = None
    page_id: str


class Views(BaseModel):
    """库的全部视图。"""
    views: List[DbView]


class Workspace(BaseModel):
    created_at: int
    icon: Optional[str] = None
    id: str
    name: str


class WsPatch(BaseModel):
    icon: Optional[str] = None
    """图标（emoji 或图片地址）；null = 去掉"""
    name: Optional[str] = None
    """新名字"""


AuthMode = Union[Literal["local", "header", "none", "jwt"], str]

DiffTo = Union[int, str]
"""对比的另一头：版本号，或 `"current"`。"""

ErrorCode = Union[Literal["not_found", "bad_request", "unauthorized", "forbidden", "conflict", "gone", "too_large", "too_many_requests", "bad_gateway", "internal"], str]
"""错误码：not_found 404、bad_request 400、unauthorized 401、forbidden 403、conflict 409、gone 410（邀请过期之类）、
too_large 413、too_many_requests 429（带 Retry-After）、bad_gateway 502（去外部网站取东西失败）、internal 500。
"""

Notify = Union[bool, List[str]]
"""评论时叫谁：false = 谁也不叫，`["agent:<id>", ...]` = 只叫这些（不给 / null = 按规则）。"""

PrincipalKind = Union[Literal["user", "agent"], str]

PropType = Union[Literal["title", "text", "number", "select", "multi_select", "status", "date", "person", "checkbox", "url", "email", "phone", "files", "created_time", "updated_time", "created_by", "updated_by", "relation", "rollup", "formula", "unique_id", "button"], str]
"""属性类型。"""

ViewObj = Dict[str, Any]
"""一个视图（DbView 的样子；id 不给或重复时分配新的，type 不给是 table，name 不给是“视图”）。"""

PutViews = Union[List[ViewObj], Dict[str, Any]]
"""换掉全部视图的正文：视图的数组，或 `{ views: [...] }`。"""

Role = Union[Literal["viewer", "editor", "admin", "owner"], str]

Scope = Union[Literal["read", "write", "admin"], str]

SignupMode = Union[Literal["invite", "open", "off"], str]
"""注册方式。"""

ViewType = Union[Literal["table", "board", "calendar", "gallery", "list"], str]
"""视图类型。"""


# 工作区事件流里，事件名 → data 的模型（没列的给原始 JSON）
EVENT_MODELS: Dict[str, Type[BaseModel]] = {
    "agent.notification": AgentNotification,
    "comment.created": CommentCreatedEvent,
    "db.changed": DbChangedEvent,
    "db.schema": DbSchemaEvent,
    "page.created": Page,
    "page.deleted": Page,
    "page.moved": Page,
    "page.purged": PagePurgedEvent,
    "page.updated": Page,
    "workspace.updated": Workspace,
}

# 互相引用的、引用后面类型别名的：都定义完了再解析一遍
for _m in (
    Ack,
    Acked,
    AddRow,
    AddRows,
    AddedRows,
    Agent,
    AgentList,
    AgentNotes,
    AgentNotification,
    AgentView,
    Anchor,
    AuthConfigBody,
    AuthorRef,
    Authors,
    BlockBody,
    BlockContent,
    BlockDiff,
    BlockIds,
    CellChange,
    CellOp,
    CommentCreatedEvent,
    CommentView,
    Content,
    CreateAgent,
    CreatePage,
    CreatedAgent,
    DbChangedEvent,
    DbHistory,
    DbHistoryItem,
    DbInfo,
    DbProp,
    DbRow,
    DbSchema,
    DbSchemaEvent,
    DbSort,
    DbUndone,
    DbView,
    Deleted,
    DocMark,
    DocNode,
    EditComment,
    ErrorBody,
    ErrorInfo,
    Ids,
    ImportForm,
    Inbox,
    InboxNotification,
    InviteBy,
    InviteInfo,
    InviteList,
    InviteView,
    InviteWorkspace,
    LoginBody,
    Me,
    MePatch,
    MeUser,
    MeWorkspace,
    Member,
    MemberList,
    MemberRole,
    MemberUser,
    NewDb,
    NewInvite,
    NewSecret,
    NewThread,
    NewToken,
    NoteAgent,
    NoteComment,
    NotePage,
    Page,
    PageList,
    PagePurgedEvent,
    PasswordBody,
    PatchAgent,
    PatchPage,
    PatchRows,
    PatchThread,
    Patched,
    Pressed,
    PutContent,
    PutShare,
    QueryGroup,
    QueryReq,
    QueryResult,
    QueryRow,
    ReadCount,
    ReadNotes,
    RelationConf,
    Reply,
    Restored,
    RestoredPage,
    RoleBody,
    RollupConf,
    RowsDelta,
    SaveBody,
    SchemaChanged,
    SchemaOps,
    SchemaUndone,
    SearchResult,
    SearchResults,
    SelectOption,
    Seq,
    ShareView,
    SignupBody,
    ThreadList,
    ThreadView,
    TrashItem,
    TrashList,
    Undo,
    UndoBody,
    UnfurlCard,
    UploadForm,
    Uploaded,
    VersionAuthor,
    VersionContent,
    VersionDiff,
    VersionList,
    VersionMeta,
    Views,
    Workspace,
    WsPatch,
):
    _m.model_rebuild()
del _m
