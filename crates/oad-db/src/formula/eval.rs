//! 算公式。
use super::{Expr, Func, MAX_LIST, MAX_STR, Op, V};
use crate::date::{self, Clock, DateTime};
use std::cmp::Ordering;

/// 算的时候要用的：这一行的属性值、现在几点。
pub trait Ctx {
    fn prop(&self, idx: usize) -> V;
    fn clock(&self) -> &Clock;
}

/// 算的时候出错（这一格是空的）。
#[derive(Debug)]
pub struct Fail;

type R = Result<V, Fail>;

/// 算一格：出错是空的；数规整、文字截断、列表变成文字的列表、空文字和空列表是空。
pub fn run(e: &Expr, ctx: &dyn Ctx) -> V {
    let mut env = Vec::new();
    let v = match eval(e, ctx, &mut env) {
        Ok(v) => v,
        Err(_) => return V::Null,
    };
    finish(v)
}

/// 算出来的值规整成存的样子。
pub fn finish(v: V) -> V {
    match v {
        V::Num(x) => crate::num::clean(x).map_or(V::Null, V::Num),
        V::Str(s) if s.is_empty() => V::Null,
        V::Str(s) if s.len() > MAX_STR => V::str(s.chars().take(MAX_STR).collect::<String>()),
        V::List(l) if l.is_empty() => V::Null,
        V::List(l) => V::list(l.iter().take(MAX_LIST).map(|x| V::str(x.text())).collect()),
        v => v,
    }
}

fn to_dt(v: &V, clock: &Clock) -> Option<DateTime> {
    match v {
        V::Date(d, _) => Some(*d),
        V::Num(ms) => Some(DateTime {
            min: clock.local_min(*ms as i64),
            time: true,
        }),
        V::Str(s) => date::parse(s, clock),
        _ => None,
    }
}

fn num_or_null(x: Option<f64>) -> V {
    x.map_or(V::Null, V::Num)
}

fn nums(args: &[V]) -> Vec<f64> {
    args.iter()
        .flat_map(|a| a.items())
        .filter_map(|x| x.num())
        .collect()
}

fn arith(op: Op, a: &V, b: &V) -> R {
    if op == Op::Add {
        if matches!(a, V::Date(..)) || matches!(b, V::Date(..)) {
            return Err(Fail); // 日期要用 dateAdd 加减
        }
        if matches!(a, V::Str(_) | V::List(_)) || matches!(b, V::Str(_) | V::List(_)) {
            return Ok(V::str(a.text() + &b.text()));
        }
    }
    let (Some(x), Some(y)) = (a.num(), b.num()) else {
        if let (Op::Sub, V::Date(x, _), V::Date(y, _)) = (op, a, b) {
            return Ok(V::Num((x.min - y.min) as f64 / 1440.0));
        }
        return Ok(V::Null);
    };
    Ok(match op {
        Op::Add => V::Num(x + y),
        Op::Sub => V::Num(x - y),
        Op::Mul => V::Num(x * y),
        Op::Div if y == 0.0 => V::Null,
        Op::Div => V::Num(x / y),
        Op::Mod if y == 0.0 => V::Null,
        Op::Mod => V::Num(x % y),
        Op::Pow => {
            if y.abs() > 1000.0 {
                return Err(Fail); // 次方太大
            }
            V::Num(x.powf(y))
        }
        _ => return Err(Fail),
    })
}

fn cmp(op: Op, a: &V, b: &V, clock: &Clock) -> bool {
    let ord = if matches!(a, V::Date(..)) || matches!(b, V::Date(..)) {
        match (to_dt(a, clock), to_dt(b, clock)) {
            (Some(x), Some(y)) => x.min.cmp(&y.min),
            _ => return op == Op::Ne,
        }
    } else if a.is_num_like() && b.is_num_like() {
        let (x, y) = (a.num().unwrap(), b.num().unwrap());
        x.partial_cmp(&y).unwrap_or(Ordering::Equal)
    } else if matches!(op, Op::Eq | Op::Ne) {
        a.text().cmp(&b.text())
    } else {
        match (a.num(), b.num()) {
            (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
            _ => a.text().cmp(&b.text()),
        }
    };
    match op {
        Op::Eq => ord == Ordering::Equal,
        Op::Ne => ord != Ordering::Equal,
        Op::Gt => ord == Ordering::Greater,
        Op::Ge => ord != Ordering::Less,
        Op::Lt => ord == Ordering::Less,
        Op::Le => ord != Ordering::Greater,
        _ => false,
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Unit {
    Years,
    Quarters,
    Months,
    Weeks,
    Days,
    Hours,
    Minutes,
}

fn unit(v: &V) -> Result<Unit, Fail> {
    Ok(match v.text().trim().to_lowercase().as_str() {
        "years" | "year" | "y" | "年" => Unit::Years,
        "quarters" | "quarter" | "季度" | "季" => Unit::Quarters,
        "months" | "month" | "月" | "个月" => Unit::Months,
        "weeks" | "week" | "w" | "周" | "星期" => Unit::Weeks,
        "days" | "day" | "d" | "天" | "日" => Unit::Days,
        "hours" | "hour" | "h" | "小时" | "时" => Unit::Hours,
        "minutes" | "minute" | "m" | "分钟" | "分" => Unit::Minutes,
        _ => return Err(Fail),
    })
}

fn months_of(u: Unit) -> Option<i64> {
    match u {
        Unit::Years => Some(12),
        Unit::Quarters => Some(3),
        Unit::Months => Some(1),
        _ => None,
    }
}

fn minutes_of(u: Unit) -> f64 {
    match u {
        Unit::Weeks => 10080.0,
        Unit::Days => 1440.0,
        Unit::Hours => 60.0,
        _ => 1.0,
    }
}

fn date_add(d: Option<DateTime>, n: Option<f64>, u: Unit) -> V {
    let (Some(d), Some(n)) = (d, n) else {
        return V::Null;
    };
    match months_of(u) {
        Some(k) => V::date(d.add_months(n.trunc() as i64 * k)),
        None => {
            let add = (n * minutes_of(u)).round() as i64;
            V::date(DateTime {
                min: d.min + add,
                time: d.time || matches!(u, Unit::Hours | Unit::Minutes) && add % 1440 != 0,
            })
        }
    }
}

/// a 减 b 有几个单位（向 0 取整）。
fn date_between(a: DateTime, b: DateTime, u: Unit) -> i64 {
    match months_of(u) {
        Some(k) => {
            if a.min < b.min {
                return -date_between(b, a, u);
            }
            let (ay, am, ad) = date::civil_from_days(a.day());
            let (by, bm, bd) = date::civil_from_days(b.day());
            let mut months = (ay - by) * 12 + am as i64 - bm as i64;
            if (ad, a.minute_of_day()) < (bd, b.minute_of_day()) {
                months -= 1;
            }
            months / k
        }
        None => ((a.min - b.min) as f64 / minutes_of(u)).trunc() as i64,
    }
}

/// Python 的切片：负数从后数，越界截掉。
fn slice_range(len: usize, a: f64, b: Option<f64>) -> (usize, usize) {
    let n = len as i64;
    let fix = |x: f64| {
        let x = x.trunc() as i64;
        (if x < 0 { (n + x).max(0) } else { x.min(n) }) as usize
    };
    let s = fix(a);
    let e = b.map_or(len, fix);
    (s, e.max(s))
}

fn sort_key_cmp(a: &V, b: &V) -> Ordering {
    match (a, b) {
        (V::Num(x), V::Num(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (V::Date(x, _), V::Date(y, _)) => x.min.cmp(&y.min),
        _ => a.text().cmp(&b.text()),
    }
}

fn median(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let n = v.len();
    Some(if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    })
}

fn pad(s: &V, len: &V, with: Option<&V>, start: bool) -> V {
    let s = s.text();
    let want = (len.num().unwrap_or(0.0).max(0.0) as usize).min(MAX_STR);
    let p = with.map_or(" ".to_string(), V::text);
    let have = s.chars().count();
    if have >= want || p.is_empty() {
        return V::str(s);
    }
    let fill: String = p.chars().cycle().take(want - have).collect();
    V::str(if start { fill + &s } else { s + &fill })
}

/// 算一个表达式。`env` 是变量（let、current、index）。
pub fn eval(e: &Expr, ctx: &dyn Ctx, env: &mut Vec<V>) -> R {
    let clock = ctx.clock();
    Ok(match e {
        Expr::Lit(v) => v.clone(),
        Expr::Prop(i) => ctx.prop(*i),
        Expr::Var(i) => env.get(*i).cloned().ok_or(Fail)?,
        Expr::Neg(a) => num_or_null(eval(a, ctx, env)?.num().map(|x| -x)),
        Expr::Not(a) => V::Bool(!eval(a, ctx, env)?.truthy()),
        Expr::If(c, a, b) => {
            if eval(c, ctx, env)?.truthy() {
                eval(a, ctx, env)?
            } else {
                eval(b, ctx, env)?
            }
        }
        Expr::Let(value, body) => {
            let v = eval(value, ctx, env)?;
            env.push(v);
            let out = eval(body, ctx, env);
            env.pop();
            out?
        }
        Expr::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for x in items {
                out.push(eval(x, ctx, env)?);
            }
            V::list(out)
        }
        Expr::Bin(op, a, b) => match op {
            Op::And => V::Bool(eval(a, ctx, env)?.truthy() && eval(b, ctx, env)?.truthy()),
            Op::Or => V::Bool(eval(a, ctx, env)?.truthy() || eval(b, ctx, env)?.truthy()),
            Op::Eq | Op::Ne | Op::Gt | Op::Ge | Op::Lt | Op::Le => {
                let (x, y) = (eval(a, ctx, env)?, eval(b, ctx, env)?);
                V::Bool(cmp(*op, &x, &y, clock))
            }
            _ => {
                let (x, y) = (eval(a, ctx, env)?, eval(b, ctx, env)?);
                arith(*op, &x, &y)?
            }
        },
        Expr::Call(f, args) => call(*f, args, ctx, env)?,
    })
}

fn call(f: Func, args: &[Expr], ctx: &dyn Ctx, env: &mut Vec<V>) -> R {
    let clock = ctx.clock();
    // 先处理不先算参数的
    match f {
        Func::Ifs => {
            for pair in args.chunks(2) {
                if pair.len() == 1 {
                    return eval(&pair[0], ctx, env);
                }
                if eval(&pair[0], ctx, env)?.truthy() {
                    return eval(&pair[1], ctx, env);
                }
            }
            return Ok(V::Null);
        }
        Func::And => {
            for a in args {
                if !eval(a, ctx, env)?.truthy() {
                    return Ok(V::Bool(false));
                }
            }
            return Ok(V::Bool(true));
        }
        Func::Or => {
            for a in args {
                if eval(a, ctx, env)?.truthy() {
                    return Ok(V::Bool(true));
                }
            }
            return Ok(V::Bool(false));
        }
        _ if f.is_lambda() => {
            let list = eval(&args[0], ctx, env)?.items();
            let mut out = vec![];
            for (i, x) in list.iter().enumerate() {
                env.push(x.clone());
                env.push(V::Num(i as f64));
                let r = eval(&args[1], ctx, env);
                env.truncate(env.len() - 2);
                let r = r?;
                match f {
                    Func::Map => out.push(r),
                    Func::Filter if r.truthy() => out.push(x.clone()),
                    Func::Find if r.truthy() => return Ok(x.clone()),
                    Func::FindIndex if r.truthy() => return Ok(V::Num(i as f64)),
                    Func::Some if r.truthy() => return Ok(V::Bool(true)),
                    Func::Every if !r.truthy() => return Ok(V::Bool(false)),
                    _ => {}
                }
            }
            return Ok(match f {
                Func::Map | Func::Filter => V::list(out),
                Func::Find => V::Null,
                Func::FindIndex => V::Num(-1.0),
                Func::Some => V::Bool(false),
                _ => V::Bool(true),
            });
        }
        _ => {}
    }
    let mut a = Vec::with_capacity(args.len());
    for x in args {
        a.push(eval(x, ctx, env)?);
    }
    let arg = |i: usize| a.get(i).cloned().unwrap_or(V::Null);
    let n0 = || a[0].num();
    let math = |g: fn(f64) -> f64| num_or_null(a[0].num().map(g));
    Ok(match f {
        Func::Not => V::Bool(!a[0].truthy()),
        Func::Empty => V::Bool(a[0].is_blank()),
        Func::Length => V::Num(match &a[0] {
            V::List(l) => l.len(),
            v => v.text().chars().count(),
        } as f64),
        Func::Concat => {
            if a.iter().all(|x| matches!(x, V::List(_))) {
                V::list(a.iter().flat_map(V::items).collect())
            } else {
                V::str(a.iter().map(V::text).collect::<String>())
            }
        }
        Func::Join => {
            let sep = arg(1).text();
            V::str(
                a[0].items()
                    .iter()
                    .map(V::text)
                    .collect::<Vec<_>>()
                    .join(&sep),
            )
        }
        Func::Contains => {
            let x = a[1].text();
            V::Bool(match &a[0] {
                V::List(l) => l.iter().any(|y| y.text() == x),
                v => v.text().contains(&x),
            })
        }
        Func::Replace => {
            let s = a[0].text().replace(&a[1].text(), &a[2].text());
            V::str(s.chars().take(MAX_STR).collect::<String>())
        }
        Func::Lower => V::str(a[0].text().to_lowercase()),
        Func::Upper => V::str(a[0].text().to_uppercase()),
        Func::Trim => V::str(a[0].text().trim()),
        Func::Slice => {
            let from = a[1].num().unwrap_or(0.0);
            let to = a.get(2).map(|x| x.num().unwrap_or(0.0));
            match &a[0] {
                V::List(l) => {
                    let (s, e) = slice_range(l.len(), from, to);
                    V::list(l[s..e].to_vec())
                }
                v => {
                    let c: Vec<char> = v.text().chars().collect();
                    let (s, e) = slice_range(c.len(), from, to);
                    V::str(c[s..e].iter().collect::<String>())
                }
            }
        }
        Func::Split => {
            let s = a[0].text();
            let parts: Vec<V> = match a.get(1) {
                None => s.split_whitespace().map(V::str).collect(),
                Some(sep) if sep.text().is_empty() => {
                    s.chars().map(|c| V::str(c.to_string())).collect()
                }
                Some(sep) => s.split(sep.text().as_str()).map(V::str).collect(),
            };
            V::list(parts)
        }
        Func::Repeat => {
            let s = a[0].text();
            let times = a[1].num().unwrap_or(0.0).max(0.0) as usize;
            let cap = if s.is_empty() {
                0
            } else {
                MAX_STR / s.chars().count() + 1
            };
            V::str(s.repeat(times.min(cap)))
        }
        Func::StartsWith => V::Bool(a[0].text().starts_with(&a[1].text())),
        Func::EndsWith => V::Bool(a[0].text().ends_with(&a[1].text())),
        Func::PadStart => pad(&a[0], &a[1], a.get(2), true),
        Func::PadEnd => pad(&a[0], &a[1], a.get(2), false),
        Func::ToNumber => num_or_null(a[0].num()),
        Func::ToText => V::str(a[0].text()),
        Func::Round => {
            let Some(x) = n0() else { return Ok(V::Null) };
            let d = a.get(1).and_then(V::num).unwrap_or(0.0).trunc() as i32;
            let q = 10f64.powi(d);
            // 四舍五入（不是银行家舍入）
            V::Num((x.abs() * q + 0.5).floor() / q * x.signum())
        }
        Func::Floor => math(f64::floor),
        Func::Ceil => math(f64::ceil),
        Func::Abs => math(f64::abs),
        Func::Sqrt => num_or_null(n0().filter(|x| *x >= 0.0).map(f64::sqrt)),
        Func::Cbrt => math(f64::cbrt),
        Func::Pow => arith(Op::Pow, &a[0], &a[1])?,
        Func::Mod => arith(Op::Mod, &a[0], &a[1])?,
        Func::Sign => math(|x| {
            if x > 0.0 {
                1.0
            } else if x < 0.0 {
                -1.0
            } else {
                0.0
            }
        }),
        Func::Exp => math(f64::exp),
        Func::Ln => num_or_null(n0().filter(|x| *x > 0.0).map(f64::ln)),
        Func::Log10 => num_or_null(n0().filter(|x| *x > 0.0).map(f64::log10)),
        Func::Log2 => num_or_null(n0().filter(|x| *x > 0.0).map(f64::log2)),
        Func::Pi => V::Num(std::f64::consts::PI),
        Func::E => V::Num(std::f64::consts::E),
        Func::Min => num_or_null(nums(&a).into_iter().reduce(f64::min)),
        Func::Max => num_or_null(nums(&a).into_iter().reduce(f64::max)),
        Func::Sum => V::Num(nums(&a).iter().sum()),
        Func::Average => {
            let v = nums(&a);
            if v.is_empty() {
                V::Null
            } else {
                V::Num(v.iter().sum::<f64>() / v.len() as f64)
            }
        }
        Func::Median => num_or_null(median(nums(&a))),
        Func::Now => V::date(clock.now()),
        Func::Today => V::date(DateTime::from_day(clock.today())),
        Func::DateAdd => date_add(to_dt(&a[0], clock), a[1].num(), unit(&a[2])?),
        Func::DateSubtract => date_add(
            to_dt(&a[0], clock),
            Some(-a[1].num().unwrap_or(0.0)),
            unit(&a[2])?,
        ),
        Func::DateBetween => {
            let u = unit(&a[2])?;
            match (to_dt(&a[0], clock), to_dt(&a[1], clock)) {
                (Some(x), Some(y)) => V::Num(date_between(x, y, u) as f64),
                _ => V::Null,
            }
        }
        Func::DateRange => match (to_dt(&a[0], clock), to_dt(&a[1], clock)) {
            (Some(x), Some(y)) => V::Date(x, Some(y)),
            (Some(x), None) => V::date(x),
            _ => V::Null,
        },
        Func::DateStart => match &a[0] {
            V::Date(s, _) => V::date(*s),
            v => to_dt(v, clock).map_or(V::Null, V::date),
        },
        Func::DateEnd => match &a[0] {
            V::Date(s, e) => V::date(e.unwrap_or(*s)),
            v => to_dt(v, clock).map_or(V::Null, V::date),
        },
        Func::FormatDate => match to_dt(&a[0], clock) {
            None => V::str(""),
            Some(d) => match a.get(1) {
                None => V::str(d.text()),
                Some(p) => V::str(date::format_pattern(&d, &p.text())),
            },
        },
        Func::ParseDate => date::parse(&a[0].text(), clock).map_or(V::Null, V::date),
        Func::Year
        | Func::Month
        | Func::Day
        | Func::Weekday
        | Func::Week
        | Func::Hour
        | Func::Minute => {
            let Some(d) = to_dt(&a[0], clock) else {
                return Ok(V::Null);
            };
            let (y, m, dd, h, mi) = d.parts();
            V::Num(match f {
                Func::Year => y as f64,
                Func::Month => m as f64,
                Func::Day => dd as f64,
                Func::Weekday => date::weekday(d.day()) as f64,
                Func::Week => date::iso_week(d.day()).1 as f64,
                Func::Hour => h as f64,
                _ => mi as f64,
            })
        }
        Func::Timestamp => match to_dt(&a[0], clock) {
            Some(d) => V::Num(clock.to_ms(d.min) as f64),
            None => V::Null,
        },
        Func::FromTimestamp => match a[0].num() {
            Some(ms) if ms.is_finite() => V::date(DateTime {
                min: clock.local_min(ms as i64),
                time: true,
            }),
            _ => V::Null,
        },
        Func::First => a[0].items().into_iter().next().unwrap_or(V::Null),
        Func::Last => a[0].items().pop().unwrap_or(V::Null),
        Func::At => {
            let l = a[0].items();
            let Some(i) = a[1].num() else {
                return Ok(V::Null);
            };
            let i = i.trunc() as i64;
            let i = if i < 0 { l.len() as i64 + i } else { i };
            if i < 0 {
                V::Null
            } else {
                l.get(i as usize).cloned().unwrap_or(V::Null)
            }
        }
        Func::Reverse => {
            let mut l = a[0].items();
            l.reverse();
            V::list(l)
        }
        Func::Sort => {
            let mut l = a[0].items();
            l.sort_by(sort_key_cmp);
            V::list(l)
        }
        Func::Unique => {
            let mut seen = rustc_hash::FxHashSet::default();
            V::list(
                a[0].items()
                    .into_iter()
                    .filter(|x| seen.insert(x.text()))
                    .collect(),
            )
        }
        Func::Flat => V::list(a[0].items().iter().flat_map(V::items).collect()),
        Func::If | Func::Ifs | Func::And | Func::Or | Func::Let | Func::Lets => return Err(Fail),
        Func::Map | Func::Filter | Func::Find | Func::FindIndex | Func::Some | Func::Every => {
            return Err(Fail);
        }
    })
}
