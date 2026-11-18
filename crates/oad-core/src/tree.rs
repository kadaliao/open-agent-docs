//! 页面树的纯逻辑：算位置、查环。
use crate::model::Placement;
use crate::position;
use crate::{Error, Result};

/// siblings 是同级（不含要放的那页）按顺序的 (id, position)。
pub fn position_for(siblings: &[(String, String)], place: &Placement) -> Result<String> {
    let pos = |i: usize| siblings.get(i).map(|s| s.1.as_str());
    Ok(match place {
        Placement::Last => position::between(siblings.last().map(|s| s.1.as_str()), None),
        Placement::First => position::between(None, pos(0)),
        Placement::After(id) => {
            let i = siblings
                .iter()
                .position(|s| &s.0 == id)
                .ok_or_else(|| Error::BadRequest(format!("after={id} 不是同级页面")))?;
            let (a, b) = (pos(i).unwrap(), pos(i + 1));
            if b == Some(a) {
                // 位置重复了（不该出现）：退到放最后
                position::between(siblings.last().map(|s| s.1.as_str()), None)
            } else {
                position::between(Some(a), b)
            }
        }
    })
}

/// ancestors 是新父页面往上一直到根的编号（含新父页面自己）。
pub fn check_cycle(id: &str, ancestors: &[String]) -> Result<()> {
    if ancestors.iter().any(|a| a == id) {
        return Err(Error::BadRequest("不能移到自己或子页面下面".into()));
    }
    Ok(())
}
