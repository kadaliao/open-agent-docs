//! 只用来写 OpenAPI：库的表结构、属性、视图的样子（真正收发的是 oad-db 里的 serde 类型和 JSON，形状和这里一样；
//! 不认识的字段原样保留，所以这里没列全的也会出现）。
#![allow(dead_code)]
use utoipa::ToSchema;

/// 表结构。
#[derive(ToSchema)]
#[schema(as = DbSchema)]
pub struct DbSchema {
    pub props: Vec<DbProp>,
    /// 下一个自动编号
    #[schema(required = false)]
    pub next_id: u64,
}

/// 属性类型。
#[derive(ToSchema)]
#[schema(as = PropType)]
#[serde(rename_all = "snake_case")]
pub enum PropType {
    Title,
    Text,
    Number,
    Select,
    MultiSelect,
    Status,
    Date,
    Person,
    Checkbox,
    Url,
    Email,
    Phone,
    Files,
    CreatedTime,
    UpdatedTime,
    CreatedBy,
    UpdatedBy,
    Relation,
    Rollup,
    Formula,
    UniqueId,
    Button,
}

/// 一个属性（一列）。`key` 是短随机串，改名不动数据；各类型自己的配置只在那个类型上有。
#[derive(ToSchema)]
#[schema(as = DbProp)]
pub struct DbProp {
    pub key: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: PropType,
    /// select / multi_select / status 的选项
    #[schema(required = false)]
    pub options: Vec<SelectOption>,
    /// 数字的格式：number | comma | percent | cny | usd（number、formula、rollup）
    #[schema(required = false)]
    pub format: String,
    /// 显示几位小数
    #[schema(required = false)]
    pub precision: u8,
    /// 日期的显示：iso | zh | slash | relative
    #[schema(required = false)]
    pub date_format: String,
    /// formula 的公式
    #[schema(required = false)]
    pub formula: String,
    #[schema(required = false)]
    pub relation: RelationConf,
    #[schema(required = false)]
    pub rollup: RollupConf,
    /// unique_id 的前缀（T-12）
    #[schema(required = false)]
    pub prefix: String,
    /// button 的文字
    #[schema(required = false)]
    pub label: String,
    /// button 按下时设的值：`{ 属性 key 或名字: 值 }`
    #[schema(required = false, value_type = Object)]
    pub set: serde_json::Value,
    /// button 按下时发的评论
    #[schema(required = false)]
    pub comment: String,
    /// button 按下时通知的 Agent（id）
    #[schema(required = false)]
    pub agent: String,
}

/// 选项。status 的有 `group`：todo / doing / done。
#[derive(ToSchema)]
#[schema(as = SelectOption)]
pub struct SelectOption {
    pub id: String,
    pub name: String,
    pub color: String,
    #[schema(required = false)]
    pub group: String,
}

/// 关联：指向哪个库；`mirror_of` 有值时是双向关联的反向那一半。
#[derive(ToSchema)]
#[schema(as = RelationConf)]
pub struct RelationConf {
    pub db_id: String,
    #[schema(required = false)]
    pub mirror_of: String,
}

/// 汇总：沿关联属性 `relation` 取对方库的属性 `prop`，按 `fn` 归总。
#[derive(ToSchema)]
#[schema(as = RollupConf)]
pub struct RollupConf {
    pub relation: String,
    pub prop: String,
    #[serde(rename = "fn")]
    pub func: String,
}

/// 视图类型。
#[derive(ToSchema)]
#[schema(as = ViewType)]
#[serde(rename_all = "snake_case")]
pub enum ViewType {
    Table,
    Board,
    Calendar,
    Gallery,
    List,
}

/// 一个视图。存的时候按表结构清理（去掉指向已删属性的排序、分组、隐藏、列宽）。
#[derive(ToSchema)]
#[schema(as = DbView)]
pub struct DbView {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: ViewType,
    /// 筛选：`{ op: "and" | "or", items: [{ prop, op, value } 或嵌套的组] }`
    #[schema(required = false, value_type = Object)]
    pub filter: serde_json::Value,
    #[schema(required = false)]
    pub sorts: Vec<DbSort>,
    /// 隐藏的属性 key
    #[schema(required = false)]
    pub hidden: Vec<String>,
    /// 列的顺序（属性 key）
    #[schema(required = false)]
    pub order: Vec<String>,
    /// 列宽：`{ key: 像素 }`
    #[schema(required = false, value_type = Object)]
    pub widths: serde_json::Value,
    /// 按哪个属性分组（看板的列）
    #[schema(required = false)]
    pub group_by: String,
    #[schema(required = false)]
    pub sub_group_by: String,
    /// 按日期分组时怎么分：day | week | month | year | relative
    #[schema(required = false)]
    pub group_date: String,
    #[schema(required = false)]
    pub sub_group_date: String,
    /// 去掉没有行的分组
    #[schema(required = false)]
    pub hide_empty_groups: bool,
    /// 日历按哪个日期属性
    #[schema(required = false)]
    pub calendar_by: String,
    /// 每列底部的汇总：`{ key: fn }`
    #[schema(required = false, value_type = Object)]
    pub totals: serde_json::Value,
}

/// 排序：`dir: "asc" | "desc"`。
#[derive(ToSchema)]
#[schema(as = DbSort)]
pub struct DbSort {
    pub prop: String,
    #[schema(required = false)]
    pub dir: String,
}
