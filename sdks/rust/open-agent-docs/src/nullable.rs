//! 请求体里可空可省的字段：不发 / 发 `null` / 发值（契约 §5）。
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// 三态的字段：
///
/// - `Omit`（默认）：不发这个字段，服务器当没改；
/// - `Null`：发 `null`（比如 `PatchPage.after: null` = 挪到最前、`icon: null` = 清掉图标）；
/// - `Value(v)`：发值。
///
/// 值可以直接 `.into()`：`title: "周报".into()`。
///
/// ```
/// use open_agent_docs::{Nullable, types::PatchPage};
/// let p = PatchPage { icon: Nullable::Null, title: "新标题".into(), ..Default::default() };
/// assert_eq!(serde_json::to_value(&p).unwrap(), serde_json::json!({ "icon": null, "title": "新标题" }));
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum Nullable<T> {
    /// 不发
    #[default]
    Omit,
    /// 发 `null`
    Null,
    /// 发值
    Value(T),
}

impl<T> Nullable<T> {
    /// 是不是 `Omit`（serde 的 `skip_serializing_if` 用）
    pub fn is_omitted(&self) -> bool {
        matches!(self, Nullable::Omit)
    }

    /// 是不是 `Null`
    pub fn is_null(&self) -> bool {
        matches!(self, Nullable::Null)
    }

    /// 有值时借出来
    pub fn as_value(&self) -> Option<&T> {
        match self {
            Nullable::Value(v) => Some(v),
            _ => None,
        }
    }

    /// 有值时拿出来
    pub fn into_value(self) -> Option<T> {
        match self {
            Nullable::Value(v) => Some(v),
            _ => None,
        }
    }

    /// `Option` → `Nullable`：`None` 是发 `null`（不是不发）
    pub fn from_option_or_null(v: Option<T>) -> Self {
        v.map_or(Nullable::Null, Nullable::Value)
    }

    /// `Option` → `Nullable`：`None` 是不发
    pub fn from_option_or_omit(v: Option<T>) -> Self {
        v.map_or(Nullable::Omit, Nullable::Value)
    }
}

impl<T> From<T> for Nullable<T> {
    fn from(v: T) -> Self {
        Nullable::Value(v)
    }
}

impl From<&str> for Nullable<String> {
    fn from(v: &str) -> Self {
        Nullable::Value(v.to_string())
    }
}

impl From<&String> for Nullable<String> {
    fn from(v: &String) -> Self {
        Nullable::Value(v.clone())
    }
}

impl<T: Serialize> Serialize for Nullable<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Nullable::Value(v) => v.serialize(s),
            // Omit 正常不会走到这里（字段上有 skip_serializing_if），单独序列化时也当 null
            _ => s.serialize_none(),
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Nullable<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // 字段没出现时 serde(default) 给 Omit；出现了是 null 或值
        Ok(Option::<T>::deserialize(d)?.map_or(Nullable::Null, Nullable::Value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Ack, CreatePage, PatchPage};
    use serde_json::json;

    #[test]
    fn three_states() {
        let omit = PatchPage::default();
        assert_eq!(serde_json::to_value(&omit).unwrap(), json!({}));
        let null = PatchPage {
            after: Nullable::Null,
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&null).unwrap(),
            json!({ "after": null })
        );
        let val = PatchPage {
            after: "p2".into(),
            parent_id: Nullable::Null,
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&val).unwrap(),
            json!({ "after": "p2", "parent_id": null })
        );
    }

    #[test]
    fn round_trip() {
        let p: PatchPage = serde_json::from_value(json!({ "after": null, "title": "x" })).unwrap();
        assert_eq!(p.after, Nullable::Null);
        assert_eq!(p.title, Nullable::Value("x".into()));
        assert_eq!(p.icon, Nullable::Omit);
    }

    #[test]
    fn numbers_and_options() {
        let a = Ack {
            upto: 42.into(),
            ..Default::default()
        };
        assert_eq!(serde_json::to_value(&a).unwrap(), json!({ "upto": 42 }));
        let a = Ack {
            ids: Some(vec![1, 2]),
            ..Default::default()
        };
        assert_eq!(serde_json::to_value(&a).unwrap(), json!({ "ids": [1, 2] }));
        let c = CreatePage {
            title: "周报".into(),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&c).unwrap(),
            json!({ "title": "周报" })
        );
    }
}
