//! 公式：写法参考 Notion。`prop("属性名")` 取这一行的值；字面量有数字、"文字"（也认 '…'、“…”、「…」）、true / false、`[甲, 乙]`；
//! 运算 `+ - * / % ^`、`== != > >= < <=`（`=` 当 `==`）、`and or not`（也认 `&& || !`）、`条件 ? 甲 : 乙`；
//! 函数见 [`FUNCS`]，也能写成方法：`prop("标签").length()` = `length(prop("标签"))`；`let(x, 值, 表达式)`、`lets(a, 1, b, 2, 表达式)`；
//! `map / filter / find / findIndex / some / every` 的第二个参数里用 `current`（当前元素）和 `index`（第几个，从 0 起）。
//!
//! 解析在改表结构时做（写错了报清楚哪里错，带位置）；算是读的时候按行算。算的时候出错（除以 0、类型不对）这一格是空的。
pub(crate) mod eval;
mod func;

pub use eval::{Ctx, eval, finish, run};
pub use func::{FUNCS, Func, FuncInfo};

use crate::date::DateTime;
use std::fmt;
use std::sync::Arc;

pub const MAX_LEN: usize = 1000; // 公式最长多少字
pub const MAX_DEPTH: usize = 40; // 嵌套多深
pub const MAX_STR: usize = 5000; // 算出来的文字最长多少
pub const MAX_LIST: usize = 200; // 算出来的列表最长多少

/// 公式里的值。
#[derive(Clone, Debug, PartialEq, Default)]
pub enum V {
    #[default]
    Null,
    Num(f64),
    Str(Arc<str>),
    Bool(bool),
    /// 开始和（日期范围的）结束
    Date(DateTime, Option<DateTime>),
    List(Arc<Vec<V>>),
}

impl V {
    pub fn str(s: impl Into<Arc<str>>) -> V {
        V::Str(s.into())
    }

    pub fn date(d: DateTime) -> V {
        V::Date(d, None)
    }

    pub fn list(v: Vec<V>) -> V {
        V::List(Arc::new(v))
    }

    /// 转成文字（toText）：空是 ""，真假是 true / false，日期 `2026-11-15 14:00`，列表用 `, ` 连起来。
    pub fn text(&self) -> String {
        match self {
            V::Null => String::new(),
            V::Num(x) => crate::num::text(*x),
            V::Str(s) => s.to_string(),
            V::Bool(b) => if *b { "true" } else { "false" }.into(),
            V::Date(d, None) => d.text(),
            V::Date(d, Some(e)) => format!("{} → {}", d.text(), e.text()),
            V::List(l) => l.iter().map(V::text).collect::<Vec<_>>().join(", "),
        }
    }

    /// 转成数：真假是 1 / 0，文字能认就认（去掉千分位），别的是 None。
    pub fn num(&self) -> Option<f64> {
        match self {
            V::Num(x) => Some(*x),
            V::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            V::Str(s) => {
                let t: String = s.chars().filter(|c| *c != ',').collect();
                let t = t.trim();
                if t.is_empty() || t.chars().any(|c| c.is_alphabetic() && !"eE".contains(c)) {
                    return None;
                }
                t.parse::<f64>().ok().filter(|x| x.is_finite())
            }
            _ => None,
        }
    }

    /// 当条件用时算不算“是”。
    pub fn truthy(&self) -> bool {
        match self {
            V::Null => false,
            V::Num(x) => *x != 0.0,
            V::Str(s) => !s.is_empty(),
            V::Bool(b) => *b,
            V::Date(..) => true,
            V::List(l) => !l.is_empty(),
        }
    }

    /// empty()：空、""、[]、false、0 都算空。
    pub fn is_blank(&self) -> bool {
        match self {
            V::Null => true,
            V::Num(x) => *x == 0.0,
            V::Str(s) => s.is_empty(),
            V::Bool(b) => !b,
            V::Date(..) => false,
            V::List(l) => l.is_empty(),
        }
    }

    /// 汇总、底部汇总里算不算“有值”：空、""、[]、false 是空（0 不算空）。
    pub fn is_none_like(&self) -> bool {
        match self {
            V::Null => true,
            V::Str(s) => s.is_empty(),
            V::Bool(b) => !b,
            V::List(l) => l.is_empty(),
            _ => false,
        }
    }

    pub fn is_num_like(&self) -> bool {
        matches!(self, V::Num(_) | V::Bool(_))
    }

    /// 展开一层列表。
    pub fn items(&self) -> Vec<V> {
        match self {
            V::List(l) => l.as_ref().clone(),
            v => vec![v.clone()],
        }
    }
}

/// 解析出错：说清楚哪里错。`at` 是出错处在公式里的字节位置。
#[derive(Clone, Debug, PartialEq)]
pub struct FormulaError {
    pub msg: String,
    pub at: usize,
}

impl FormulaError {
    pub fn new(msg: impl Into<String>, at: usize) -> FormulaError {
        FormulaError {
            msg: msg.into(),
            at,
        }
    }

    /// 出错处是第几个字（从 1 起）。
    pub fn char_pos(&self, src: &str) -> usize {
        src.get(..self.at.min(src.len()))
            .map_or(0, |s| s.chars().count())
            + 1
    }

    /// 出错处在 JS 字符串里的下标（UTF-16）。
    pub fn utf16_pos(&self, src: &str) -> usize {
        src.get(..self.at.min(src.len()))
            .map_or(0, |s| s.encode_utf16().count())
    }

    /// 给人看的：`公式第 3 个字：……`
    pub fn describe(&self, src: &str) -> String {
        format!("公式第 {} 个字：{}", self.char_pos(src), self.msg)
    }
}

impl fmt::Display for FormulaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.msg)
    }
}

impl std::error::Error for FormulaError {}

/// 运算符。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Or,
    And,
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
}

/// 解析好的公式。
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Lit(V),
    /// 属性（解析时按名字找到的序号）
    Prop(usize),
    /// 变量（let、current、index），按定义的先后编号
    Var(usize),
    Neg(Box<Expr>),
    Not(Box<Expr>),
    Bin(Op, Box<Expr>, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    Call(Func, Vec<Expr>),
    /// let：先算值、放进变量，再算后面
    Let(Box<Expr>, Box<Expr>),
    List(Vec<Expr>),
}

/// 解析结果：表达式、用到的属性、用没用到 now() / today()（用到了时间一变就要重算）。
#[derive(Clone, Debug, PartialEq)]
pub struct Compiled {
    pub expr: Expr,
    pub refs: Vec<usize>,
    pub uses_clock: bool,
}

// ---------- 词法 ----------

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Str(String),
    Name(String),
    Op(&'static str),
    End,
}

#[derive(Clone, Debug)]
struct Token {
    tok: Tok,
    at: usize,
    end: usize,
}

fn tokens(src: &str) -> Result<Vec<Token>, FormulaError> {
    let mut out = vec![];
    let b: Vec<(usize, char)> = src.char_indices().collect();
    let mut i = 0;
    let pos = |i: usize| b.get(i).map_or(src.len(), |x| x.0);
    while i < b.len() {
        let (at, c) = b[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() {
            let st = i;
            while i < b.len() && b[i].1.is_ascii_digit() {
                i += 1;
            }
            if i + 1 < b.len() && b[i].1 == '.' && b[i + 1].1.is_ascii_digit() {
                i += 1;
                while i < b.len() && b[i].1.is_ascii_digit() {
                    i += 1;
                }
            }
            if i < b.len() && (b[i].1 == 'e' || b[i].1 == 'E') {
                let mut j = i + 1;
                if j < b.len() && (b[j].1 == '+' || b[j].1 == '-') {
                    j += 1;
                }
                if j < b.len() && b[j].1.is_ascii_digit() {
                    while j < b.len() && b[j].1.is_ascii_digit() {
                        j += 1;
                    }
                    i = j;
                }
            }
            let text = &src[pos(st)..pos(i)];
            let n: f64 = text
                .parse()
                .map_err(|_| FormulaError::new(format!("看不懂的数「{text}」"), at))?;
            out.push(Token {
                tok: Tok::Num(n),
                at,
                end: pos(i),
            });
            continue;
        }
        let close = match c {
            '"' => Some('"'),
            '\'' => Some('\''),
            '“' => Some('”'),
            '‘' => Some('’'),
            '「' => Some('」'),
            _ => None,
        };
        if let Some(close) = close {
            let escapes = c == '"' || c == '\'';
            let mut s = String::new();
            i += 1;
            loop {
                let Some(&(_, ch)) = b.get(i) else {
                    return Err(FormulaError::new(
                        "引号没有配对：这段文字少了结束的引号",
                        at,
                    ));
                };
                i += 1;
                if ch == close {
                    break;
                }
                if escapes
                    && ch == '\\'
                    && let Some(&(_, e)) = b.get(i)
                {
                    i += 1;
                    s.push(match e {
                        'n' => '\n',
                        't' => '\t',
                        x => x,
                    });
                    continue;
                }
                s.push(ch);
            }
            out.push(Token {
                tok: Tok::Str(s),
                at,
                end: pos(i),
            });
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let st = i;
            while i < b.len() && (b[i].1.is_alphanumeric() || b[i].1 == '_') {
                i += 1;
            }
            out.push(Token {
                tok: Tok::Name(src[pos(st)..pos(i)].to_string()),
                at,
                end: pos(i),
            });
            continue;
        }
        let next = b.get(i + 1).map(|x| x.1);
        let two = match (c, next) {
            ('=', Some('=')) => Some("=="),
            ('!', Some('=')) => Some("!="),
            ('>', Some('=')) => Some(">="),
            ('<', Some('=')) => Some("<="),
            ('&', Some('&')) => Some("&&"),
            ('|', Some('|')) => Some("||"),
            ('<', Some('>')) => Some("!="),
            _ => None,
        };
        if let Some(op) = two {
            i += 2;
            out.push(Token {
                tok: Tok::Op(op),
                at,
                end: pos(i),
            });
            continue;
        }
        let one = match c {
            '-' | '－' => "-",
            '+' | '＋' => "+",
            '*' | '×' => "*",
            '/' | '÷' => "/",
            '%' => "%",
            '^' => "^",
            '(' | '（' => "(",
            ')' | '）' => ")",
            '[' | '【' => "[",
            ']' | '】' => "]",
            '<' => "<",
            '>' => ">",
            '!' | '！' => "!",
            '?' | '？' => "?",
            ':' | '：' => ":",
            ',' | '，' => ",",
            '=' => "==",
            '≠' => "!=",
            '≥' => ">=",
            '≤' => "<=",
            '.' => ".",
            _ => {
                let near: String = src[at..].chars().take(12).collect();
                return Err(FormulaError::new(format!("这里看不懂：{near}"), at));
            }
        };
        i += 1;
        out.push(Token {
            tok: Tok::Op(one),
            at,
            end: pos(i),
        });
    }
    out.push(Token {
        tok: Tok::End,
        at: src.len(),
        end: src.len(),
    });
    Ok(out)
}

fn tok_text(t: &Token, src: &str) -> String {
    match t.tok {
        Tok::End => "结尾".into(),
        _ => src[t.at..t.end].to_string(),
    }
}

fn bin_op(t: &Tok) -> Option<(Op, u8)> {
    let s = match t {
        Tok::Op(s) => *s,
        Tok::Name(n) => match n.to_lowercase().as_str() {
            "or" => "||",
            "and" => "&&",
            _ => return None,
        },
        _ => return None,
    };
    Some(match s {
        "||" => (Op::Or, 1),
        "&&" => (Op::And, 2),
        "==" => (Op::Eq, 3),
        "!=" => (Op::Ne, 3),
        ">" => (Op::Gt, 4),
        ">=" => (Op::Ge, 4),
        "<" => (Op::Lt, 4),
        "<=" => (Op::Le, 4),
        "+" => (Op::Add, 5),
        "-" => (Op::Sub, 5),
        "*" => (Op::Mul, 6),
        "/" => (Op::Div, 6),
        "%" => (Op::Mod, 6),
        "^" => (Op::Pow, 8),
        _ => return None,
    })
}

// ---------- 语法 ----------

/// 按名字找属性：找到给序号，找不到给报错的话。
pub type Resolve<'a> = dyn Fn(&str) -> Result<usize, String> + 'a;

struct Parser<'a> {
    src: &'a str,
    t: Vec<Token>,
    i: usize,
    depth: usize,
    scope: Vec<String>,
    resolve: &'a Resolve<'a>,
    refs: Vec<usize>,
    uses_clock: bool,
}

impl Parser<'_> {
    fn peek(&self) -> &Token {
        &self.t[self.i]
    }

    fn is_op(&self, op: &str) -> bool {
        matches!(self.peek().tok, Tok::Op(o) if o == op)
    }

    fn take(&mut self) -> Token {
        let t = self.t[self.i].clone();
        if self.i + 1 < self.t.len() {
            self.i += 1;
        }
        t
    }

    fn expect(&mut self, op: &str) -> Result<(), FormulaError> {
        if self.is_op(op) {
            self.take();
            return Ok(());
        }
        let t = self.peek();
        let msg = if t.tok == Tok::End {
            format!("公式没写完：这里少了「{op}」")
        } else {
            format!("这里应该是「{op}」，不是「{}」", tok_text(t, self.src))
        };
        Err(FormulaError::new(msg, t.at))
    }

    fn expr(&mut self) -> Result<Expr, FormulaError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(FormulaError::new("公式嵌套太深", self.peek().at));
        }
        let cond = self.binary(1)?;
        let out = if self.is_op("?") {
            self.take();
            let a = self.expr()?;
            self.expect(":")?;
            let b = self.expr()?;
            Expr::If(Box::new(cond), Box::new(a), Box::new(b))
        } else {
            cond
        };
        self.depth -= 1;
        Ok(out)
    }

    fn binary(&mut self, level: u8) -> Result<Expr, FormulaError> {
        let mut left = self.unary()?;
        loop {
            let Some((op, prec)) = bin_op(&self.peek().tok) else {
                return Ok(left);
            };
            if prec < level {
                return Ok(left);
            }
            self.take();
            if self.peek().tok == Tok::End {
                return Err(FormulaError::new("公式没写完", self.peek().at));
            }
            // ^ 右结合
            let right = self.binary(if op == Op::Pow { prec } else { prec + 1 })?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
    }

    fn unary(&mut self) -> Result<Expr, FormulaError> {
        let t = self.peek().clone();
        let kind = match &t.tok {
            Tok::Op("-") => Some('-'),
            Tok::Op("+") => Some('+'),
            Tok::Op("!") => Some('!'),
            Tok::Name(n) if n.eq_ignore_ascii_case("not") => {
                // not(…) 是函数，后面不跟括号是运算符；两种结果一样
                Some('!')
            }
            _ => None,
        };
        let Some(k) = kind else {
            return self.postfix();
        };
        self.take();
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(FormulaError::new("公式嵌套太深", t.at));
        }
        // 比 * / 紧，比 ^ 松：-2^2 = -4
        let node = self.binary(7)?;
        self.depth -= 1;
        Ok(match k {
            '-' => Expr::Neg(Box::new(node)),
            '!' => Expr::Not(Box::new(node)),
            _ => node,
        })
    }

    fn postfix(&mut self) -> Result<Expr, FormulaError> {
        let mut node = self.atom()?;
        while self.is_op(".") {
            self.take();
            let t = self.take();
            let Tok::Name(name) = &t.tok else {
                return Err(FormulaError::new(
                    "「.」后面应该是函数名，比如 .length()",
                    t.at,
                ));
            };
            let Some(info) = func::lookup(name) else {
                return Err(unknown_func(name, t.at));
            };
            if !self.is_op("(") {
                return Err(FormulaError::new(
                    format!("「.{name}」后面要跟括号：.{}()", info.name),
                    self.peek().at,
                ));
            }
            node = self.call(info, t.at, Some(node))?;
        }
        Ok(node)
    }

    /// 函数调用：`(` 已经在眼前。`recv` 是方法写法里点前面的那个值（当第一个参数）。
    fn call(
        &mut self,
        info: &'static FuncInfo,
        at: usize,
        recv: Option<Expr>,
    ) -> Result<Expr, FormulaError> {
        self.expect("(")?;
        let mut args: Vec<Expr> = recv.into_iter().collect();
        let lambda = info.func.is_lambda();
        let binds = matches!(info.func, Func::Let | Func::Lets);
        let mut pushed = 0;
        if !self.is_op(")") {
            loop {
                let n = args.len();
                if binds && n.is_multiple_of(2) && self.has_more_after_arg() {
                    // 变量名
                    let t = self.take();
                    let Tok::Name(var) = &t.tok else {
                        return Err(FormulaError::new(
                            format!("{} 的变量名要写成名字，比如 let(x, 1, x + 1)", info.name),
                            t.at,
                        ));
                    };
                    if !self.is_op(",") {
                        return Err(FormulaError::new(
                            format!("{} 的变量名后面要跟「,」和值", info.name),
                            self.peek().at,
                        ));
                    }
                    args.push(Expr::Lit(V::str(var.as_str())));
                    self.take();
                    let value = self.expr()?;
                    args.push(value);
                    self.scope.push(var.clone());
                    pushed += 1;
                    if !self.is_op(",") {
                        break;
                    }
                    self.take();
                    continue;
                }
                if lambda && n == 1 {
                    self.scope.push("current".into());
                    self.scope.push("index".into());
                    pushed += 2;
                }
                args.push(self.expr()?);
                if self.is_op(",") {
                    self.take();
                    continue;
                }
                break;
            }
        }
        self.scope.truncate(self.scope.len() - pushed);
        self.expect(")")?;
        let n = args.len();
        if binds && n % 2 == 1 && pushed != n / 2 {
            return Err(FormulaError::new(
                format!("{} 的变量名要写成名字，比如 let(x, 1, x + 1)", info.name),
                at,
            ));
        }
        if n < info.min as usize || (info.max != u8::MAX && n > info.max as usize) {
            return Err(FormulaError::new(
                format!("{} 的参数个数不对（{}）", info.name, info.usage),
                at,
            ));
        }
        if matches!(info.func, Func::Now | Func::Today) {
            self.uses_clock = true;
        }
        Ok(match info.func {
            Func::Let | Func::Lets => {
                if n.is_multiple_of(2) {
                    return Err(FormulaError::new(
                        format!("{} 的参数个数不对（{}）", info.name, info.usage),
                        at,
                    ));
                }
                // let(a, 1, b, 2, 体) → Let(1, Let(2, 体))；名字只在解析时用
                let mut it = args.into_iter().rev();
                let mut body = it.next().unwrap();
                let rest: Vec<Expr> = it.collect();
                for pair in rest.chunks(2) {
                    body = Expr::Let(Box::new(pair[0].clone()), Box::new(body));
                }
                body
            }
            Func::If => {
                let mut it = args.into_iter();
                Expr::If(
                    Box::new(it.next().unwrap()),
                    Box::new(it.next().unwrap()),
                    Box::new(it.next().unwrap()),
                )
            }
            f => Expr::Call(f, args),
        })
    }

    /// let 的参数里：现在这个后面还有没有别的参数（有的话现在这个是变量名，没有就是最后的表达式）。
    fn has_more_after_arg(&self) -> bool {
        matches!(self.peek().tok, Tok::Name(_))
            && matches!(self.t.get(self.i + 1).map(|t| &t.tok), Some(Tok::Op(",")))
    }

    fn atom(&mut self) -> Result<Expr, FormulaError> {
        let t = self.take();
        match t.tok {
            Tok::End => Err(FormulaError::new("公式没写完", t.at)),
            Tok::Num(n) => Ok(Expr::Lit(V::Num(n))),
            Tok::Str(s) => Ok(Expr::Lit(V::str(s))),
            Tok::Op("(") => {
                let node = self.expr()?;
                self.expect(")")?;
                Ok(node)
            }
            Tok::Op("[") => {
                let mut items = vec![];
                if !self.is_op("]") {
                    loop {
                        items.push(self.expr()?);
                        if self.is_op(",") {
                            self.take();
                            continue;
                        }
                        break;
                    }
                }
                self.expect("]")?;
                Ok(Expr::List(items))
            }
            Tok::Name(ref name) => {
                let low = name.to_lowercase();
                let call = self.is_op("(");
                if !call {
                    if low == "true" || low == "false" {
                        return Ok(Expr::Lit(V::Bool(low == "true")));
                    }
                    if let Some(i) = self.scope.iter().rposition(|v| *v == *name) {
                        return Ok(Expr::Var(i));
                    }
                    return Err(FormulaError::new(
                        format!("「{name}」不是属性也不是变量：取属性的值写 prop(\"{name}\")"),
                        t.at,
                    ));
                }
                if low == "prop" {
                    self.take();
                    let a = self.take();
                    let Tok::Str(pname) = &a.tok else {
                        return Err(FormulaError::new(
                            "prop 里写属性名，比如 prop(\"数量\")",
                            a.at,
                        ));
                    };
                    if !self.is_op(")") {
                        return Err(FormulaError::new(
                            "prop 里只写一个属性名，比如 prop(\"数量\")",
                            self.peek().at,
                        ));
                    }
                    self.take();
                    let idx = (self.resolve)(pname).map_err(|m| FormulaError::new(m, a.at))?;
                    if !self.refs.contains(&idx) {
                        self.refs.push(idx);
                    }
                    return Ok(Expr::Prop(idx));
                }
                let Some(info) = func::lookup(name) else {
                    return Err(unknown_func(name, t.at));
                };
                self.call(info, t.at, None)
            }
            _ => Err(FormulaError::new(
                format!("这里多了「{}」", tok_text(&t, self.src)),
                t.at,
            )),
        }
    }
}

fn unknown_func(name: &str, at: usize) -> FormulaError {
    let low = name.to_lowercase();
    let best = FUNCS
        .iter()
        .map(|f| (edit_distance(&low, &f.name.to_lowercase()), f.name))
        .min();
    let hint = match best {
        // 短名字只容一个字的差别（foo 不该猜成 floor）
        Some((d, n))
            if d <= if low.chars().count() <= 4 { 1 } else { 2 }
                || n.to_lowercase().starts_with(&low) =>
        {
            format!("，是不是 {n}？")
        }
        _ => String::new(),
    };
    FormulaError::new(format!("没有函数「{name}」{hint}"), at)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            cur.push(
                (prev[j] + usize::from(ca != cb))
                    .min(prev[j + 1] + 1)
                    .min(cur[j] + 1),
            );
        }
        prev = cur;
    }
    prev[b.len()]
}

/// 解析一个公式。`resolve` 按名字找属性。
pub fn parse(src: &str, resolve: &Resolve) -> Result<Compiled, FormulaError> {
    let trimmed = src.trim_end();
    if trimmed.trim().is_empty() {
        return Err(FormulaError::new("公式是空的", 0));
    }
    if trimmed.chars().count() > MAX_LEN {
        return Err(FormulaError::new(
            format!("公式太长（最多 {MAX_LEN} 字）"),
            0,
        ));
    }
    let mut p = Parser {
        src: trimmed,
        t: tokens(trimmed)?,
        i: 0,
        depth: 0,
        scope: vec![],
        resolve,
        refs: vec![],
        uses_clock: false,
    };
    let expr = p.expr()?;
    if p.peek().tok != Tok::End {
        let t = p.peek();
        return Err(FormulaError::new(
            format!("公式多了「{}」", tok_text(t, trimmed)),
            t.at,
        ));
    }
    Ok(Compiled {
        expr,
        refs: p.refs,
        uses_clock: p.uses_clock,
    })
}

/// 公式里用到的属性名（按出现的先后，去重）。语法错了报错。
pub fn refs(src: &str) -> Result<Vec<String>, FormulaError> {
    let names = std::cell::RefCell::new(Vec::<String>::new());
    parse(src, &|n: &str| {
        let mut v = names.borrow_mut();
        let low = n.trim().to_lowercase();
        Ok(match v.iter().position(|x| x.to_lowercase() == low) {
            Some(i) => i,
            None => {
                v.push(n.to_string());
                v.len() - 1
            }
        })
    })?;
    Ok(names.into_inner())
}

/// 属性改名时把公式里的 `prop("旧名")` 换成新名字（不分大小写）；别的原样。公式看不懂就原样返回。
pub fn rename_prop(src: &str, old: &str, new: &str) -> String {
    let Ok(t) = tokens(src) else {
        return src.to_string();
    };
    let old = old.trim().to_lowercase();
    let mut out = String::new();
    let mut last = 0;
    for i in 0..t.len().saturating_sub(3) {
        let is_prop = matches!(&t[i].tok, Tok::Name(n) if n.eq_ignore_ascii_case("prop"));
        if is_prop
            && t[i + 1].tok == Tok::Op("(")
            && matches!(&t[i + 2].tok, Tok::Str(s) if s.trim().to_lowercase() == old)
            && t[i + 3].tok == Tok::Op(")")
        {
            out.push_str(&src[last..t[i + 2].at]);
            out.push('"');
            out.push_str(&new.replace('\\', "\\\\").replace('"', "\\\""));
            out.push('"');
            last = t[i + 2].end;
        }
    }
    out.push_str(&src[last..]);
    out
}
