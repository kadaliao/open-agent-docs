//! 公式里能用的函数：名字、参数个数、写法（报错和补全用）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Func {
    // 逻辑
    If,
    Ifs,
    And,
    Or,
    Not,
    Empty,
    Let,
    Lets,
    // 文字
    Length,
    Concat,
    Join,
    Contains,
    Replace,
    Lower,
    Upper,
    Trim,
    Slice,
    Split,
    Repeat,
    StartsWith,
    EndsWith,
    PadStart,
    PadEnd,
    ToNumber,
    ToText,
    // 数
    Round,
    Floor,
    Ceil,
    Abs,
    Sqrt,
    Cbrt,
    Pow,
    Mod,
    Sign,
    Exp,
    Ln,
    Log10,
    Log2,
    Pi,
    E,
    Min,
    Max,
    Sum,
    Average,
    Median,
    // 日期
    Now,
    Today,
    DateAdd,
    DateSubtract,
    DateBetween,
    DateRange,
    DateStart,
    DateEnd,
    FormatDate,
    ParseDate,
    Year,
    Month,
    Day,
    Weekday,
    Week,
    Hour,
    Minute,
    Timestamp,
    FromTimestamp,
    // 列表
    First,
    Last,
    At,
    Reverse,
    Sort,
    Unique,
    Flat,
    Map,
    Filter,
    Find,
    FindIndex,
    Some,
    Every,
}

impl Func {
    /// 第二个参数是对每个元素算的（里面能用 current、index）。
    pub fn is_lambda(self) -> bool {
        matches!(
            self,
            Func::Map | Func::Filter | Func::Find | Func::FindIndex | Func::Some | Func::Every
        )
    }
}

pub struct FuncInfo {
    /// 推荐的写法（不分大小写）
    pub name: &'static str,
    pub func: Func,
    pub min: u8,
    /// u8::MAX 是不限
    pub max: u8,
    pub usage: &'static str,
    /// 也认的别名
    pub aliases: &'static [&'static str],
}

const N: u8 = u8::MAX;

macro_rules! f {
    ($name:literal, $f:ident, $min:expr, $max:expr, $usage:literal) => {
        FuncInfo { name: $name, func: Func::$f, min: $min, max: $max, usage: $usage, aliases: &[] }
    };
    ($name:literal, $f:ident, $min:expr, $max:expr, $usage:literal, $($a:literal),+) => {
        FuncInfo { name: $name, func: Func::$f, min: $min, max: $max, usage: $usage, aliases: &[$($a),+] }
    };
}

pub static FUNCS: &[FuncInfo] = &[
    f!("if", If, 3, 3, "if(条件, 是的话, 不是的话)"),
    f!(
        "ifs",
        Ifs,
        3,
        N,
        "ifs(条件1, 值1, 条件2, 值2, …, 都不是的话)"
    ),
    f!("and", And, 1, N, "and(甲, 乙, …)"),
    f!("or", Or, 1, N, "or(甲, 乙, …)"),
    f!("not", Not, 1, 1, "not(甲)"),
    f!("empty", Empty, 1, 1, "empty(值)", "isEmpty"),
    f!("let", Let, 3, 3, "let(名字, 值, 表达式)"),
    f!(
        "lets",
        Lets,
        3,
        N,
        "lets(名字1, 值1, 名字2, 值2, …, 表达式)"
    ),
    f!("length", Length, 1, 1, "length(文字或列表)", "len"),
    f!("concat", Concat, 1, N, "concat(甲, 乙, …)"),
    f!("join", Join, 1, 2, "join(列表, 分隔)"),
    f!(
        "contains",
        Contains,
        2,
        2,
        "contains(文字或列表, 找什么)",
        "includes"
    ),
    f!(
        "replace",
        Replace,
        3,
        3,
        "replace(文字, 旧, 新)",
        "replaceAll"
    ),
    f!("lower", Lower, 1, 1, "lower(文字)"),
    f!("upper", Upper, 1, 1, "upper(文字)"),
    f!("trim", Trim, 1, 1, "trim(文字)"),
    f!(
        "slice",
        Slice,
        2,
        3,
        "slice(文字或列表, 从, 到)",
        "substring"
    ),
    f!("split", Split, 1, 2, "split(文字, 分隔)"),
    f!("repeat", Repeat, 2, 2, "repeat(文字, 次数)"),
    f!("startsWith", StartsWith, 2, 2, "startsWith(文字, 开头)"),
    f!("endsWith", EndsWith, 2, 2, "endsWith(文字, 结尾)"),
    f!("padStart", PadStart, 2, 3, "padStart(文字, 长度, 补什么)"),
    f!("padEnd", PadEnd, 2, 3, "padEnd(文字, 长度, 补什么)"),
    f!("toNumber", ToNumber, 1, 1, "toNumber(值)"),
    f!("toText", ToText, 1, 1, "toText(值)", "format"),
    f!("round", Round, 1, 2, "round(数, 小数位)"),
    f!("floor", Floor, 1, 1, "floor(数)"),
    f!("ceil", Ceil, 1, 1, "ceil(数)"),
    f!("abs", Abs, 1, 1, "abs(数)"),
    f!("sqrt", Sqrt, 1, 1, "sqrt(数)"),
    f!("cbrt", Cbrt, 1, 1, "cbrt(数)"),
    f!("pow", Pow, 2, 2, "pow(底, 次方)"),
    f!("mod", Mod, 2, 2, "mod(被除数, 除数)"),
    f!("sign", Sign, 1, 1, "sign(数)"),
    f!("exp", Exp, 1, 1, "exp(数)"),
    f!("ln", Ln, 1, 1, "ln(数)"),
    f!("log10", Log10, 1, 1, "log10(数)"),
    f!("log2", Log2, 1, 1, "log2(数)"),
    f!("pi", Pi, 0, 0, "pi()"),
    f!("e", E, 0, 0, "e()"),
    f!("min", Min, 1, N, "min(甲, 乙, …)"),
    f!("max", Max, 1, N, "max(甲, 乙, …)"),
    f!("sum", Sum, 1, N, "sum(甲, 乙, …)", "add"),
    f!(
        "average",
        Average,
        1,
        N,
        "average(甲, 乙, …)",
        "mean",
        "avg"
    ),
    f!("median", Median, 1, N, "median(甲, 乙, …)"),
    f!("now", Now, 0, 0, "now()"),
    f!("today", Today, 0, 0, "today()"),
    f!("dateAdd", DateAdd, 3, 3, "dateAdd(日期, 数, \"days\")"),
    f!(
        "dateSubtract",
        DateSubtract,
        3,
        3,
        "dateSubtract(日期, 数, \"days\")"
    ),
    f!(
        "dateBetween",
        DateBetween,
        3,
        3,
        "dateBetween(日期1, 日期2, \"days\")：日期1 减 日期2"
    ),
    f!("dateRange", DateRange, 2, 2, "dateRange(开始, 结束)"),
    f!("dateStart", DateStart, 1, 1, "dateStart(日期)", "start"),
    f!("dateEnd", DateEnd, 1, 1, "dateEnd(日期)", "end"),
    f!(
        "formatDate",
        FormatDate,
        1,
        2,
        "formatDate(日期, \"YYYY-MM-DD\")"
    ),
    f!("parseDate", ParseDate, 1, 1, "parseDate(\"2026-11-15\")"),
    f!("year", Year, 1, 1, "year(日期)"),
    f!("month", Month, 1, 1, "month(日期)"),
    f!("day", Day, 1, 1, "day(日期)：几号", "date"),
    f!(
        "weekday",
        Weekday,
        1,
        1,
        "weekday(日期)：周一是 1，周日是 7"
    ),
    f!("week", Week, 1, 1, "week(日期)：一年里第几周（ISO）"),
    f!("hour", Hour, 1, 1, "hour(日期)"),
    f!("minute", Minute, 1, 1, "minute(日期)"),
    f!("timestamp", Timestamp, 1, 1, "timestamp(日期)：毫秒时间戳"),
    f!(
        "fromTimestamp",
        FromTimestamp,
        1,
        1,
        "fromTimestamp(毫秒时间戳)"
    ),
    f!("first", First, 1, 1, "first(列表)"),
    f!("last", Last, 1, 1, "last(列表)"),
    f!("at", At, 2, 2, "at(列表, 第几个)：从 0 起，负数从后数"),
    f!("reverse", Reverse, 1, 1, "reverse(列表)"),
    f!("sort", Sort, 1, 1, "sort(列表)"),
    f!("unique", Unique, 1, 1, "unique(列表)"),
    f!("flat", Flat, 1, 1, "flat(列表)"),
    f!("map", Map, 2, 2, "map(列表, current * 2)"),
    f!("filter", Filter, 2, 2, "filter(列表, current > 1)"),
    f!("find", Find, 2, 2, "find(列表, current > 1)"),
    f!("findIndex", FindIndex, 2, 2, "findIndex(列表, current > 1)"),
    f!("some", Some, 2, 2, "some(列表, current > 1)"),
    f!("every", Every, 2, 2, "every(列表, current > 1)"),
];

/// 按名字找函数（不分大小写，别名也认）。
pub fn lookup(name: &str) -> Option<&'static FuncInfo> {
    FUNCS.iter().find(|f| {
        f.name.eq_ignore_ascii_case(name) || f.aliases.iter().any(|a| a.eq_ignore_ascii_case(name))
    })
}
