//! 表结构、行、视图、增量改动：存的 / 传的样子（serde）。不认识的字段原样留着（`extra`），来回不丢。
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// 属性类型。
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
    /// 不认识的类型：当成没有值
    #[serde(other)]
    Unknown,
}

impl PropType {
    pub const ALL: [PropType; 22] = [
        PropType::Title,
        PropType::Text,
        PropType::Number,
        PropType::Select,
        PropType::MultiSelect,
        PropType::Status,
        PropType::Date,
        PropType::Person,
        PropType::Checkbox,
        PropType::Url,
        PropType::Email,
        PropType::Phone,
        PropType::Files,
        PropType::CreatedTime,
        PropType::UpdatedTime,
        PropType::CreatedBy,
        PropType::UpdatedBy,
        PropType::Relation,
        PropType::Rollup,
        PropType::Formula,
        PropType::UniqueId,
        PropType::Button,
    ];

    pub fn key(self) -> &'static str {
        use PropType::*;
        match self {
            Title => "title",
            Text => "text",
            Number => "number",
            Select => "select",
            MultiSelect => "multi_select",
            Status => "status",
            Date => "date",
            Person => "person",
            Checkbox => "checkbox",
            Url => "url",
            Email => "email",
            Phone => "phone",
            Files => "files",
            CreatedTime => "created_time",
            UpdatedTime => "updated_time",
            CreatedBy => "created_by",
            UpdatedBy => "updated_by",
            Relation => "relation",
            Rollup => "rollup",
            Formula => "formula",
            UniqueId => "unique_id",
            Button => "button",
            Unknown => "unknown",
        }
    }

    /// 中文名（给人、给 Agent 看）。
    pub fn zh(self) -> &'static str {
        use PropType::*;
        match self {
            Title => "名称",
            Text => "文本",
            Number => "数字",
            Select => "单选",
            MultiSelect => "多选",
            Status => "状态",
            Date => "日期",
            Person => "人员",
            Checkbox => "勾选",
            Url => "网址",
            Email => "邮箱",
            Phone => "电话",
            Files => "文件",
            CreatedTime => "创建时间",
            UpdatedTime => "最后编辑时间",
            CreatedBy => "创建人",
            UpdatedBy => "最后编辑人",
            Relation => "关联",
            Rollup => "汇总",
            Formula => "公式",
            UniqueId => "自动编号",
            Button => "按钮",
            Unknown => "未知",
        }
    }

    /// key、中文名、英文名都认（Agent 写类型时用），不分大小写。
    pub fn from_name(s: &str) -> Option<PropType> {
        let s = s.trim().to_lowercase();
        let en = |t: PropType| t.key().replace('_', " ");
        PropType::ALL
            .into_iter()
            .find(|t| t.key() == s || t.zh() == s || en(*t) == s)
            .or(match s.as_str() {
                "multiselect" | "tags" | "标签" => Some(PropType::MultiSelect),
                "person" | "people" | "user" | "人" => Some(PropType::Person),
                "bool" | "boolean" | "复选框" => Some(PropType::Checkbox),
                "link" | "链接" => Some(PropType::Url),
                "file" | "附件" => Some(PropType::Files),
                "id" | "编号" => Some(PropType::UniqueId),
                "name" => Some(PropType::Title),
                _ => None,
            })
    }

    /// 读的时候算、不存的类型（反向关联另看 [`Prop::is_mirror`]）。
    pub fn is_calculated(self) -> bool {
        matches!(self, PropType::Formula | PropType::Rollup)
    }

    /// 由行本身的信息自动给的（创建时间、创建人…）。
    pub fn is_auto(self) -> bool {
        matches!(
            self,
            PropType::CreatedTime
                | PropType::UpdatedTime
                | PropType::CreatedBy
                | PropType::UpdatedBy
                | PropType::UniqueId
        )
    }

    pub fn is_textual(self) -> bool {
        matches!(
            self,
            PropType::Title | PropType::Text | PropType::Url | PropType::Email | PropType::Phone
        )
    }

    pub fn has_options(self) -> bool {
        matches!(
            self,
            PropType::Select | PropType::MultiSelect | PropType::Status
        )
    }
}

/// 选项。status 的有 `group`：todo / doing / done。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct SelectOption {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// 关联：指向哪个库；`mirror_of` 有值时是双向关联在这边的反向那一半（值不存，读的时候从 `db_id` 那个库的 `mirror_of` 属性反查）。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct RelationConf {
    #[serde(alias = "db")]
    pub db_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mirror_of: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// 汇总：沿这个库的关联属性 `relation` 取对方库的属性 `prop`，按 `fn` 归总。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct RollupConf {
    pub relation: String,
    #[serde(alias = "target", default)]
    pub prop: String,
    #[serde(rename = "fn", default)]
    pub func: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// 一个属性（一列）。`key` 是短随机串，改名不动数据。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Prop {
    pub key: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: PropType,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<SelectOption>,
    /// 数字的格式：number | comma | percent | cny | usd（公式、汇总算出数字时也用）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// 数字显示几位小数（不给就按值本身）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precision: Option<u8>,
    /// 日期的显示：iso（2026-11-15，默认）| zh（2026年11月15日）| slash（2026/11/15）| relative（今天、明天…）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formula: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation: Option<RelationConf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollup: Option<RollupConf>,
    /// 自动编号的前缀（T-12）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Prop {
    pub fn new(key: &str, name: &str, kind: PropType) -> Prop {
        Prop {
            key: key.into(),
            name: name.into(),
            kind,
            options: vec![],
            format: None,
            precision: None,
            date_format: None,
            formula: None,
            relation: None,
            rollup: None,
            prefix: None,
            extra: Map::new(),
        }
    }

    /// 双向关联的反向那一半。
    pub fn is_mirror(&self) -> bool {
        self.kind == PropType::Relation
            && self
                .relation
                .as_ref()
                .is_some_and(|r| r.mirror_of.as_deref().is_some_and(|m| !m.is_empty()))
    }

    /// 读的时候算的：公式、汇总、反向关联。
    pub fn is_computed(&self) -> bool {
        self.kind.is_calculated() || self.is_mirror()
    }

    /// 能直接写值的。
    pub fn is_writable(&self) -> bool {
        !self.is_computed()
            && !self.kind.is_auto()
            && !matches!(self.kind, PropType::Button | PropType::Unknown)
    }
}

/// 表结构。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Schema {
    #[serde(default)]
    pub props: Vec<Prop>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_id: Option<u64>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Schema {
    pub fn prop(&self, key: &str) -> Option<&Prop> {
        self.props.iter().find(|p| p.key == key)
    }

    /// 按 key 找，找不到再按名字（不分大小写）。
    pub fn find(&self, r: &str) -> Option<&Prop> {
        let r = r.trim();
        self.prop(r).or_else(|| {
            let low = r.to_lowercase();
            self.props.iter().find(|p| p.name.to_lowercase() == low)
        })
    }
}

/// 一行（加载、同步时给的）。`props` 是存的样子，空值不存。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct RowData {
    pub id: String,
    #[serde(default)]
    pub seq: i64,
    #[serde(default)]
    pub props: Map<String, Value>,
    /// 毫秒时间戳
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<i64>,
}

/// 增量改动：`row` 整行换掉（同步来的）；`props` 只改给了的格子（null / 空是清空）；`deleted` 删行。
/// 没有这一行时 `row` / `props` 是加一行（排在最后）。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Change {
    pub row_id: String,
    #[serde(default, alias = "set", skip_serializing_if = "Option::is_none")]
    pub props: Option<Map<String, Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row: Option<RowData>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deleted: bool,
}

/// 人：`user:<id>` / `agent:<id>` 和名字。规范化时按名字认，显示时写名字。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Person {
    pub id: String,
    pub name: String,
}

/// 视图里和查询有关的部分（别的字段原样留在 `extra`）。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct View {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<Value>,
    #[serde(default, alias = "sort", skip_serializing_if = "Vec::is_empty")]
    pub sorts: Vec<Sort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_group_by: Option<String>,
    /// 按日期分组时怎么分：day | week | month（默认）| year | relative
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_group_date: Option<String>,
    /// 去掉没有行的分组（默认选项都列出来，看板上空的列也在）
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hide_empty_groups: bool,
    /// 每列底部的汇总：{ key: fn }
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub totals: Map<String, Value>,
    /// 本人临时的搜索（不存进视图）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// 排序：`dir: "asc" | "desc"`，也认 `desc: true`。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Sort {
    pub prop: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desc: Option<bool>,
}

impl Sort {
    pub fn is_desc(&self) -> bool {
        self.desc.unwrap_or(false)
            || self
                .dir
                .as_deref()
                .is_some_and(|d| d.eq_ignore_ascii_case("desc"))
    }
}
