//! 归总：汇总属性（沿关联取对方一列）和每列底部的汇总都用这里。
use crate::date::DateTime;
use crate::formula::V;
use crate::num;

/// 归总的算法。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Agg {
    /// 显示原值（列表）
    Show,
    /// 去重显示
    ShowUnique,
    /// 条数（关联了几行 / 这一页几行）
    Count,
    /// 有几个值（多选、人员按个数算）
    CountValues,
    /// 不同的值有几个
    CountUnique,
    CountEmpty,
    CountNotEmpty,
    PercentEmpty,
    PercentNotEmpty,
    Sum,
    Average,
    Median,
    Min,
    Max,
    Range,
    Earliest,
    Latest,
    /// 最早到最晚（日期范围）
    DateRange,
    Checked,
    Unchecked,
    PercentChecked,
    PercentUnchecked,
}

const ALL: &[(Agg, &str, &str, &[&str])] = &[
    (
        Agg::Show,
        "show",
        "显示原值",
        &["show_original", "original", "原值"],
    ),
    (
        Agg::ShowUnique,
        "show_unique",
        "去重显示",
        &["unique_values"],
    ),
    (Agg::Count, "count", "条数", &["count_all", "计数", "行数"]),
    (Agg::CountValues, "count_values", "有值的个数", &["values"]),
    (
        Agg::CountUnique,
        "count_unique",
        "不同值的个数",
        &["unique", "去重计数"],
    ),
    (Agg::CountEmpty, "count_empty", "空的个数", &["empty", "空"]),
    (
        Agg::CountNotEmpty,
        "count_not_empty",
        "不空的个数",
        &["not_empty", "非空"],
    ),
    (Agg::PercentEmpty, "percent_empty", "空的占比", &[]),
    (Agg::PercentNotEmpty, "percent_not_empty", "不空的占比", &[]),
    (Agg::Sum, "sum", "求和", &["total", "合计", "总和"]),
    (Agg::Average, "average", "平均", &["avg", "mean", "平均值"]),
    (Agg::Median, "median", "中位数", &[]),
    (Agg::Min, "min", "最小", &["最小值"]),
    (Agg::Max, "max", "最大", &["最大值"]),
    (Agg::Range, "range", "最大减最小", &["范围"]),
    (Agg::Earliest, "earliest", "最早", &["earliest_date"]),
    (Agg::Latest, "latest", "最晚", &["latest_date"]),
    (Agg::DateRange, "date_range", "日期范围", &[]),
    (Agg::Checked, "checked", "勾了几个", &["count_checked"]),
    (
        Agg::Unchecked,
        "unchecked",
        "没勾几个",
        &["count_unchecked"],
    ),
    (Agg::PercentChecked, "percent_checked", "勾了的占比", &[]),
    (
        Agg::PercentUnchecked,
        "percent_unchecked",
        "没勾的占比",
        &[],
    ),
];

impl Agg {
    /// key、中文名、英文写法（空格当下划线）、别名都认，不分大小写。
    pub fn parse(s: &str) -> Option<Agg> {
        let s = s.trim().to_lowercase().replace([' ', '-'], "_");
        ALL.iter()
            .find(|(_, k, zh, al)| *k == s || *zh == s || al.contains(&s.as_str()))
            .map(|x| x.0)
    }

    pub fn key(self) -> &'static str {
        ALL.iter().find(|x| x.0 == self).unwrap().1
    }

    pub fn zh(self) -> &'static str {
        ALL.iter().find(|x| x.0 == self).unwrap().2
    }

    pub fn all() -> impl Iterator<Item = Agg> {
        ALL.iter().map(|x| x.0)
    }

    /// 算出来是不是占比（0–1，显示成百分比）。
    pub fn is_percent(self) -> bool {
        matches!(
            self,
            Agg::PercentEmpty | Agg::PercentNotEmpty | Agg::PercentChecked | Agg::PercentUnchecked
        )
    }

    /// 算出来是不是个数（显示时不套属性的数字格式）。
    pub fn is_count(self) -> bool {
        matches!(
            self,
            Agg::Count
                | Agg::CountValues
                | Agg::CountUnique
                | Agg::CountEmpty
                | Agg::CountNotEmpty
                | Agg::Checked
                | Agg::Unchecked
        )
    }
}

fn ratio(a: usize, total: usize) -> V {
    if total == 0 {
        V::Null
    } else {
        V::Num(num::clean(a as f64 / total as f64).unwrap_or(0.0))
    }
}

/// `values` 是每一行那一列在公式里的样子（列表是多选、人员这些），`total` 是几行。
pub fn aggregate(f: Agg, values: &[V], total: usize) -> V {
    let filled = values.iter().filter(|v| !v.is_none_like()).count();
    let flat = || {
        values
            .iter()
            .flat_map(V::items)
            .filter(|v| !v.is_none_like())
    };
    let nums = || -> Vec<f64> {
        values
            .iter()
            .flat_map(V::items)
            .filter(|v| !matches!(v, V::Date(..)))
            .filter_map(|v| v.num())
            .collect()
    };
    let dates = || -> Vec<DateTime> {
        let mut d: Vec<DateTime> = values
            .iter()
            .flat_map(V::items)
            .filter_map(|v| match v {
                V::Date(s, _) => Some(s),
                _ => None,
            })
            .collect();
        d.sort();
        d
    };
    let checked = values.iter().filter(|v| matches!(v, V::Bool(true))).count();
    let n = |x: usize| V::Num(x as f64);
    let numeric = |g: &dyn Fn(&[f64]) -> f64| {
        let v = nums();
        if v.is_empty() {
            V::Null
        } else {
            num::clean(g(&v)).map_or(V::Null, V::Num)
        }
    };
    match f {
        Agg::Show => {
            let v: Vec<V> = flat().map(|x| V::str(x.text())).collect();
            if v.is_empty() { V::Null } else { V::list(v) }
        }
        Agg::ShowUnique => {
            let mut seen = rustc_hash::FxHashSet::default();
            let v: Vec<V> = flat()
                .map(|x| x.text())
                .filter(|t| seen.insert(t.clone()))
                .map(V::str)
                .collect();
            if v.is_empty() { V::Null } else { V::list(v) }
        }
        Agg::Count => n(total),
        Agg::CountValues => n(flat().count()),
        Agg::CountUnique => {
            let set: rustc_hash::FxHashSet<String> = flat().map(|x| x.text()).collect();
            n(set.len())
        }
        Agg::CountEmpty => n(total.saturating_sub(filled)),
        Agg::CountNotEmpty => n(filled),
        Agg::PercentEmpty => ratio(total.saturating_sub(filled), total),
        Agg::PercentNotEmpty => ratio(filled, total),
        Agg::Checked => n(checked),
        Agg::Unchecked => n(total.saturating_sub(checked)),
        Agg::PercentChecked => ratio(checked, total),
        Agg::PercentUnchecked => ratio(total.saturating_sub(checked), total),
        Agg::Earliest | Agg::Latest | Agg::DateRange => {
            let d = dates();
            match (d.first(), d.last()) {
                (Some(a), Some(b)) => match f {
                    Agg::Earliest => V::date(*a),
                    Agg::Latest => V::date(*b),
                    _ => V::Date(*a, Some(*b)),
                },
                _ => V::Null,
            }
        }
        Agg::Range => {
            let d = dates();
            if let (Some(a), Some(b)) = (d.first(), d.last()) {
                // 日期：差几天
                return V::Num(((b.min - a.min) as f64 / 1440.0 * 100.0).round() / 100.0);
            }
            numeric(&|v| {
                v.iter().cloned().fold(f64::MIN, f64::max)
                    - v.iter().cloned().fold(f64::MAX, f64::min)
            })
        }
        Agg::Sum => numeric(&|v| v.iter().sum()),
        Agg::Average => numeric(&|v| v.iter().sum::<f64>() / v.len() as f64),
        Agg::Median => numeric(&|v| {
            let mut v = v.to_vec();
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let k = v.len();
            if k % 2 == 1 {
                v[k / 2]
            } else {
                (v[k / 2 - 1] + v[k / 2]) / 2.0
            }
        }),
        Agg::Min => numeric(&|v| v.iter().cloned().fold(f64::MAX, f64::min)),
        Agg::Max => numeric(&|v| v.iter().cloned().fold(f64::MIN, f64::max)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: &str) -> V {
        V::str(x)
    }

    #[test]
    fn names() {
        assert_eq!(Agg::parse("求和"), Some(Agg::Sum));
        assert_eq!(Agg::parse("Percent checked"), Some(Agg::PercentChecked));
        assert_eq!(Agg::parse("show_original"), Some(Agg::Show));
        assert_eq!(Agg::parse("avg"), Some(Agg::Average));
        assert_eq!(Agg::parse("unique"), Some(Agg::CountUnique));
        assert_eq!(Agg::parse("not_empty"), Some(Agg::CountNotEmpty));
        assert_eq!(Agg::parse("什么"), None);
        for a in Agg::all() {
            assert_eq!(Agg::parse(a.key()), Some(a));
            assert_eq!(Agg::parse(a.zh()), Some(a));
        }
    }

    #[test]
    fn numbers() {
        let v = [V::Num(5.0), V::Num(8.0), V::Null, V::Num(1.0)];
        assert_eq!(aggregate(Agg::Sum, &v, 4), V::Num(14.0));
        assert_eq!(
            aggregate(Agg::Average, &v, 4),
            V::Num(num::clean(14.0 / 3.0).unwrap())
        );
        assert_eq!(aggregate(Agg::Median, &v, 4), V::Num(5.0));
        assert_eq!(aggregate(Agg::Min, &v, 4), V::Num(1.0));
        assert_eq!(aggregate(Agg::Max, &v, 4), V::Num(8.0));
        assert_eq!(aggregate(Agg::Range, &v, 4), V::Num(7.0));
        assert_eq!(aggregate(Agg::Count, &v, 4), V::Num(4.0));
        assert_eq!(aggregate(Agg::CountValues, &v, 4), V::Num(3.0));
        assert_eq!(aggregate(Agg::CountEmpty, &v, 4), V::Num(1.0));
        assert_eq!(aggregate(Agg::PercentEmpty, &v, 4), V::Num(0.25));
        assert_eq!(aggregate(Agg::Sum, &[V::Null], 1), V::Null);
        assert_eq!(
            aggregate(Agg::Sum, &[V::Num(0.1), V::Num(0.2)], 2),
            V::Num(0.3)
        );
    }

    #[test]
    fn lists_and_checks() {
        let tags = [
            V::list(vec![s("甲"), s("乙")]),
            V::list(vec![s("乙")]),
            V::Null,
        ];
        assert_eq!(aggregate(Agg::CountValues, &tags, 3), V::Num(3.0));
        assert_eq!(aggregate(Agg::CountUnique, &tags, 3), V::Num(2.0));
        assert_eq!(
            aggregate(Agg::Show, &tags, 3),
            V::list(vec![s("甲"), s("乙"), s("乙")])
        );
        assert_eq!(
            aggregate(Agg::ShowUnique, &tags, 3),
            V::list(vec![s("甲"), s("乙")])
        );
        let checks = [V::Bool(true), V::Bool(false), V::Bool(true), V::Bool(false)];
        assert_eq!(aggregate(Agg::Checked, &checks, 4), V::Num(2.0));
        assert_eq!(aggregate(Agg::PercentChecked, &checks, 4), V::Num(0.5));
        assert_eq!(aggregate(Agg::PercentUnchecked, &checks, 4), V::Num(0.5));
        assert_eq!(aggregate(Agg::PercentChecked, &[], 0), V::Null);
        assert_eq!(aggregate(Agg::CountEmpty, &checks, 4), V::Num(2.0));
    }

    #[test]
    fn dates() {
        let d = |y, m, dd| V::date(DateTime::ymd(y, m, dd));
        let v = [d(2026, 11, 20), d(2026, 11, 15), V::Null];
        assert_eq!(aggregate(Agg::Earliest, &v, 3), d(2026, 11, 15));
        assert_eq!(aggregate(Agg::Latest, &v, 3), d(2026, 11, 20));
        assert_eq!(aggregate(Agg::Range, &v, 3), V::Num(5.0));
        assert_eq!(
            aggregate(Agg::DateRange, &v, 3),
            V::Date(
                DateTime::ymd(2026, 11, 15),
                Some(DateTime::ymd(2026, 11, 20))
            )
        );
        assert_eq!(aggregate(Agg::Sum, &v, 3), V::Null); // 日期不当数加
    }
}
