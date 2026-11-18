"""数据库里算出来的属性（dbs.py 用）：公式、关联（含另一个库里显示的反向关联）、汇总。

- 公式：写法参考 Notion。prop("属性名") 取这一行的值，字面量有数字、"文字"、true / false，
  运算有 + - * / % ^、== != > >= < <=、and or not（也认 && || !）、条件 ? 甲 : 乙，函数见 FUNCS。
  文字和别的东西相加是拼起来。日期在公式里是 datetime，算出来是日期的显示成日期。
  解析在改表结构时做（写错了直接报错，说清楚哪里错）；算是读的时候按行算，不存。算的时候出错（比如除以 0）这一格是空的。
- 关联：值是另一个库（可以是自己）里的行号列表，存在行的 props 里。双向的关联在对方库里有一个反向属性（mirror_of），
  它的值不存，读的时候从这边反查；在反向那边改，改的是这边的行。
- 汇总：沿一个关联拿到对方的行，取其中一个属性，按 fn 归总（条数、求和、平均、最早……）。
"""
import math
import re
import statistics
from datetime import date, datetime, timedelta
from . import i18n
from .i18n import N_

MAX_LEN = 1000      # 公式最长多少字
MAX_DEPTH = 40      # 嵌套多深
MAX_STR = 5000      # 算出来的文字最长多少
MAX_CHAIN = 4       # 公式 / 汇总互相引用最多几层（多了当成环）


class FormulaError(ValueError):
    pass


# ---------- 词法、语法 ----------
_TOKEN = re.compile(r"""\s*(?:
    (?P<num>\d+(?:\.\d+)?(?:[eE][+-]?\d+)?) |
    (?P<str>"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|“[^”]*”|「[^」]*」) |
    (?P<name>[A-Za-z_一-鿿][A-Za-z0-9_一-鿿]*) |
    (?P<op>==|!=|>=|<=|&&|\|\||[-+*/%^()<>!?:,=]|，)
)""", re.X)


def _tokens(src: str) -> list[tuple[str, str]]:
    out, i = [], 0
    src = src.rstrip()
    while i < len(src):
        m = _TOKEN.match(src, i)
        if not m or m.end() == i:
            raise FormulaError(i18n.t("公式第 {n} 个字附近看不懂：{text}", n=i + 1, text=src[i:i + 12]))
        i = m.end()
        kind = m.lastgroup
        text = m.group(kind)
        if kind == "op" and text == "，":
            text = ","
        if kind == "op" and text == "=":
            text = "=="
        if kind == "str":
            if text[0] in "\"'":
                text = re.sub(r"\\(.)", lambda x: {"n": "\n", "t": "\t"}.get(x.group(1), x.group(1)), text[1:-1])
            else:
                text = text[1:-1]
        out.append((kind, text))
    out.append(("end", ""))
    return out


_BIN = {"or": 1, "||": 1, "and": 2, "&&": 2, "==": 3, "!=": 3, ">": 4, ">=": 4, "<": 4, "<=": 4,
        "+": 5, "-": 5, "*": 6, "/": 6, "%": 6, "^": 8}


class _Parser:
    def __init__(self, src: str):
        self.t = _tokens(src)
        self.i = 0
        self.depth = 0

    def peek(self):
        return self.t[self.i]

    def take(self, text=None):
        tok = self.t[self.i]
        if text is not None and tok[1] != text:
            got = tok[1] or i18n.t("结尾")
            raise FormulaError(i18n.t("公式这里应该是「{want}」，不是「{got}」", want=text, got=got))
        self.i += 1
        return tok

    def parse(self):
        node = self.expr()
        if self.peek()[0] != "end":
            raise FormulaError(i18n.t("公式多了「{text}」", text=self.peek()[1]))
        return node

    def expr(self):
        self.depth += 1
        if self.depth > MAX_DEPTH:
            raise FormulaError(i18n.t("公式嵌套太深"))
        cond = self.binary(1)
        if self.peek() == ("op", "?"):
            self.take()
            a = self.expr()
            self.take(":")
            b = self.expr()
            cond = ("call", "if", [cond, a, b])
        self.depth -= 1
        return cond

    def binary(self, level):
        left = self.unary()
        while True:
            kind, text = self.peek()
            op = text.lower() if kind == "name" else text if kind == "op" else None
            if op not in _BIN or _BIN[op] < level:
                return left
            self.take()
            right = self.binary(_BIN[op] + (0 if op == "^" else 1))   # ^ 右结合
            left = ("bin", {"||": "or", "&&": "and"}.get(op, op), left, right)

    def unary(self):
        kind, text = self.peek()
        if (kind == "op" and text in ("-", "+", "!")) or (kind == "name" and text.lower() == "not"):
            self.take()
            node = self.binary(7)   # 比 * / 紧，比 ^ 松：-2^2 = -4
            return ("neg", node) if text == "-" else ("not", node) if text != "+" else node
        return self.atom()

    def atom(self):
        kind, text = self.take()
        if kind == "end":
            raise FormulaError(i18n.t("公式没写完"))
        if kind == "num":
            return ("lit", float(text) if any(c in text for c in ".eE") else int(text))
        if kind == "str":
            return ("lit", text)
        if kind == "op" and text == "(":
            node = self.expr()
            self.take(")")
            return node
        if kind == "name":
            low = text.lower()
            if low in ("true", "false") and self.peek() != ("op", "("):
                return ("lit", low == "true")
            if self.peek() != ("op", "("):
                raise FormulaError(i18n.t("「{name}」后面要跟括号；取属性的值写 prop(\"{name}\")", name=text))
            self.take("(")
            args = []
            if self.peek() != ("op", ")"):
                while True:
                    args.append(self.expr())
                    if self.peek() == ("op", ","):
                        self.take()
                        continue
                    break
            self.take(")")
            if low == "prop":
                if len(args) != 1 or args[0][0] != "lit" or not isinstance(args[0][1], str):
                    raise FormulaError(i18n.t('prop 里写属性名，比如 prop("数量")'))
                return ("prop", args[0][1])
            if low not in FUNCS:
                raise FormulaError(i18n.t("没有函数 {name}，能用的有：{funcs}", name=text, funcs=", ".join(sorted(FUNCS))))
            lo, hi = FUNCS[low][1]
            if len(args) < lo or (hi is not None and len(args) > hi):
                raise FormulaError(i18n.t("{name} 的参数个数不对（{usage}）", name=low, usage=i18n.t(FUNCS[low][2])))
            return ("call", low, args)
        raise FormulaError(i18n.t("公式这里多了「{text}」", text=text or i18n.t("结尾")))


def parse(src: str):
    src = str(src or "").strip()
    if not src:
        raise FormulaError(i18n.t("公式是空的"))
    if len(src) > MAX_LEN:
        raise FormulaError(i18n.t("公式太长（最多 {n} 字）", n=MAX_LEN))
    return _Parser(src).parse()


def refs(node) -> set[str]:
    """公式里用到的属性名。"""
    if node[0] == "prop":
        return {node[1]}
    out = set()
    for x in node[1:]:
        if isinstance(x, tuple):
            out |= refs(x)
        elif isinstance(x, list):
            for y in x:
                out |= refs(y)
    return out


# ---------- 值 ----------
def to_dt(v):
    """日期属性的值（{start}、"2026-11-15 14:00"）或 datetime → datetime；认不出返回 None。"""
    if isinstance(v, datetime):
        return v
    if isinstance(v, date):
        return datetime(v.year, v.month, v.day)
    if isinstance(v, dict):
        v = v.get("start")
    if isinstance(v, (int, float)) and not isinstance(v, bool):
        return datetime.fromtimestamp(v)
    if isinstance(v, str):
        for fmt in ("%Y-%m-%d %H:%M", "%Y-%m-%d"):
            try:
                return datetime.strptime(v.strip()[:16], fmt)
            except ValueError:
                pass
    return None


def text(v) -> str:
    if v is None:
        return ""
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, float):
        return str(int(v)) if v.is_integer() and abs(v) < 1e15 else f"{v:.10g}"
    if isinstance(v, datetime):
        return v.strftime("%Y-%m-%d %H:%M") if (v.hour or v.minute) else v.strftime("%Y-%m-%d")
    if isinstance(v, list):
        return ", ".join(text(x) for x in v)
    return str(v)


def num(v):
    if isinstance(v, bool):
        return 1 if v else 0
    if isinstance(v, (int, float)):
        return v
    if v is None or v == "":
        return None
    if isinstance(v, list):
        return None
    try:
        n = float(str(v).replace(",", "").strip())
        return int(n) if n.is_integer() and abs(n) < 2 ** 53 else n
    except ValueError:
        return None


def _truth(v) -> bool:
    if isinstance(v, str):
        return v != ""
    if isinstance(v, list):
        return bool(v)
    return bool(v)


def _empty(v) -> bool:
    return v is None or v == "" or v == [] or v is False or (isinstance(v, (int, float)) and not isinstance(v, bool) and v == 0)


def _clean(v):
    """算出来的值：数字规整一下，NaN、无穷当成错。"""
    if isinstance(v, float):
        if v != v or v in (math.inf, -math.inf):
            raise FormulaError(i18n.t("算出来不是一个数"))
        return int(v) if v.is_integer() and abs(v) < 2 ** 53 else round(v, 10)
    if isinstance(v, str) and len(v) > MAX_STR:
        return v[:MAX_STR]
    return v


def _arith(op, a, b):
    if op == "+":
        if isinstance(a, datetime) or isinstance(b, datetime):
            raise FormulaError(i18n.t("日期要用 dateAdd 加减"))
        if isinstance(a, str) or isinstance(b, str) or isinstance(a, list) or isinstance(b, list):
            return text(a) + text(b)
    x, y = num(a), num(b)
    if x is None or y is None:
        if op == "-" and isinstance(a, datetime) and isinstance(b, datetime):
            return (a - b).total_seconds() / 86400
        return None
    if op == "+":
        return x + y
    if op == "-":
        return x - y
    if op == "*":
        return x * y
    if op == "/":
        return None if y == 0 else x / y
    if op == "%":
        return None if y == 0 else math.fmod(x, y)
    if op == "^":
        if abs(y) > 1000:
            raise FormulaError(i18n.t("次方太大"))
        return x ** y
    raise FormulaError(op)


def _cmp(op, a, b):
    if isinstance(a, datetime) or isinstance(b, datetime):
        a, b = to_dt(a), to_dt(b)
        if a is None or b is None:
            return op == "!="
    elif isinstance(a, (int, float)) and isinstance(b, (int, float)):
        pass
    elif op in ("==", "!="):
        a, b = text(a), text(b)
    else:
        na, nb = num(a), num(b)
        a, b = (na, nb) if na is not None and nb is not None else (text(a), text(b))
    return {"==": a == b, "!=": a != b, ">": a > b, ">=": a >= b, "<": a < b, "<=": a <= b}[op]


_UNITS = {"years": "years", "year": "years", "年": "years", "quarters": "quarters", "季度": "quarters", "months": "months", "month": "months",
          "月": "months", "weeks": "weeks", "week": "weeks", "周": "weeks", "days": "days", "day": "days", "天": "days", "日": "days",
          "quarter": "quarters", "hours": "hours", "hour": "hours", "小时": "hours", "minutes": "minutes", "minute": "minutes", "分钟": "minutes"}


def _unit(u) -> str:
    u = _UNITS.get(str(u or "").strip().lower())
    if not u:
        raise FormulaError(i18n.t("时间单位写 years、months、weeks、days、hours、minutes（或者 年、月、周、天、小时、分钟）"))
    return u


def _add_months(d: datetime, n: int) -> datetime:
    m = d.month - 1 + n
    y, m = d.year + m // 12, m % 12 + 1
    last = [31, 29 if y % 4 == 0 and (y % 100 or y % 400 == 0) else 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][m - 1]
    return d.replace(year=y, month=m, day=min(d.day, last))


def _date_add(d, n, u):
    d, n, u = to_dt(d), num(n), _unit(u)
    if d is None or n is None:
        return None
    if u in ("years", "quarters", "months"):
        return _add_months(d, int(n) * {"years": 12, "quarters": 3, "months": 1}[u])
    return d + timedelta(**{u: n})


def _date_between(a, b, u):
    a, b, u = to_dt(a), to_dt(b), _unit(u)
    if a is None or b is None:
        return None
    if u in ("years", "quarters", "months"):
        months = (a.year - b.year) * 12 + a.month - b.month - (1 if (a.day, a.time()) < (b.day, b.time()) and a > b else 0)
        return months // {"years": 12, "quarters": 3, "months": 1}[u] if months >= 0 else -((-months) // {"years": 12, "quarters": 3, "months": 1}[u])
    secs = (a - b).total_seconds()
    per = {"weeks": 604800, "days": 86400, "hours": 3600, "minutes": 60}[u]
    return int(secs / per)


def _format_date(d, fmt=None):
    d = to_dt(d)
    if d is None:
        return ""
    if fmt is None:
        return text(d)
    f = str(fmt)
    for a, b in (("YYYY", "%Y"), ("MM", "%m"), ("DD", "%d"), ("HH", "%H"), ("mm", "%M"), ("ss", "%S")):
        f = f.replace(a, b)
    return d.strftime(f)


def _round(x, digits=0):
    x, digits = num(x), num(digits)
    if x is None:
        return None
    digits = int(digits or 0)
    q = 10 ** digits
    return math.floor(abs(x) * q + 0.5) / q * (1 if x >= 0 else -1)   # 四舍五入（不是银行家舍入）


def _flat(args):
    out = []
    for a in args:
        out.extend(a if isinstance(a, list) else [a])
    return out


def _nums(args):
    return [n for n in (num(x) for x in _flat(args)) if n is not None]


def _slice(s, a, b=None):
    s = text(s) if not isinstance(s, list) else s
    a = int(num(a) or 0)
    b = None if b is None else int(num(b) or 0)
    return s[a:b]


# 函数：名字 → (实现, (最少参数, 最多参数 / None 不限), 写法)。写法里有中文的包上 N_，报错时再翻
FUNCS = {
    "if": (None, (3, 3), N_("if(条件, 是的话, 不是的话)")),
    "ifs": (None, (3, None), N_("ifs(条件1, 值1, 条件2, 值2, …, 都不是的话)")),
    "and": (None, (1, None), N_("and(甲, 乙, …)")),
    "or": (None, (1, None), N_("or(甲, 乙, …)")),
    "not": (lambda v: not _truth(v), (1, 1), N_("not(甲)")),
    "empty": (lambda v: _empty(v), (1, 1), N_("empty(值)")),
    "length": (lambda v: len(v) if isinstance(v, list) else len(text(v)), (1, 1), N_("length(文字或列表)")),
    "concat": (lambda *a: "".join(text(x) for x in a), (1, None), N_("concat(甲, 乙, …)")),
    "join": (lambda lst, sep="": text(sep).join(text(x) for x in (lst if isinstance(lst, list) else [lst])), (1, 2), N_("join(列表, 分隔)")),
    "contains": (lambda s, x: (text(x) in [text(y) for y in s]) if isinstance(s, list) else text(x) in text(s), (2, 2), N_("contains(文字或列表, 找什么)")),
    "replace": (lambda s, a, b: text(s).replace(text(a), text(b)), (3, 3), N_("replace(文字, 旧, 新)")),
    "lower": (lambda s: text(s).lower(), (1, 1), N_("lower(文字)")),
    "upper": (lambda s: text(s).upper(), (1, 1), N_("upper(文字)")),
    "trim": (lambda s: text(s).strip(), (1, 1), N_("trim(文字)")),
    "slice": (_slice, (2, 3), N_("slice(文字, 从, 到)")),
    "tonumber": (lambda v: num(v), (1, 1), N_("toNumber(值)")),
    "totext": (lambda v: text(v), (1, 1), N_("toText(值)")),
    "format": (lambda v: text(v), (1, 1), N_("format(值)")),
    "round": (_round, (1, 2), N_("round(数, 小数位)")),
    "floor": (lambda x: None if num(x) is None else math.floor(num(x)), (1, 1), N_("floor(数)")),
    "ceil": (lambda x: None if num(x) is None else math.ceil(num(x)), (1, 1), N_("ceil(数)")),
    "abs": (lambda x: None if num(x) is None else abs(num(x)), (1, 1), N_("abs(数)")),
    "sqrt": (lambda x: None if num(x) is None or num(x) < 0 else math.sqrt(num(x)), (1, 1), N_("sqrt(数)")),
    "pow": (lambda x, y: _arith("^", x, y), (2, 2), N_("pow(底, 次方)")),
    "min": (lambda *a: min(_nums(a)) if _nums(a) else None, (1, None), N_("min(甲, 乙, …)")),
    "max": (lambda *a: max(_nums(a)) if _nums(a) else None, (1, None), N_("max(甲, 乙, …)")),
    "sum": (lambda *a: sum(_nums(a)), (1, None), N_("sum(甲, 乙, …)")),
    "average": (lambda *a: statistics.fmean(_nums(a)) if _nums(a) else None, (1, None), N_("average(甲, 乙, …)")),
    "now": (lambda: datetime.now().replace(second=0, microsecond=0), (0, 0), "now()"),
    "today": (lambda: datetime.combine(date.today(), datetime.min.time()), (0, 0), "today()"),
    "dateadd": (_date_add, (3, 3), N_('dateAdd(日期, 数, "days")')),
    "datesubtract": (lambda d, n, u: _date_add(d, -(num(n) or 0), u), (3, 3), N_('dateSubtract(日期, 数, "days")')),
    "datebetween": (_date_between, (3, 3), N_('dateBetween(日期1, 日期2, "days")：日期1 减 日期2')),
    "formatdate": (_format_date, (1, 2), N_('formatDate(日期, "YYYY-MM-DD")')),
    "parsedate": (lambda s: to_dt(text(s)), (1, 1), 'parseDate("2026-11-15")'),
    "year": (lambda d: to_dt(d).year if to_dt(d) else None, (1, 1), N_("year(日期)")),
    "month": (lambda d: to_dt(d).month if to_dt(d) else None, (1, 1), N_("month(日期)")),
    "day": (lambda d: to_dt(d).day if to_dt(d) else None, (1, 1), N_("day(日期)")),
    "weekday": (lambda d: to_dt(d).isoweekday() if to_dt(d) else None, (1, 1), N_("weekday(日期)：周一是 1，周日是 7")),
    "hour": (lambda d: to_dt(d).hour if to_dt(d) else None, (1, 1), N_("hour(日期)")),
}


def evaluate(node, get):
    """get(属性名) → 这一行这个属性在公式里的值（找不到属性抛 FormulaError）。"""
    k = node[0]
    if k == "lit":
        return node[1]
    if k == "prop":
        return get(node[1])
    if k == "neg":
        v = num(evaluate(node[1], get))
        return None if v is None else -v
    if k == "not":
        return not _truth(evaluate(node[1], get))
    if k == "bin":
        op = node[1]
        if op == "and":
            return _truth(evaluate(node[2], get)) and _truth(evaluate(node[3], get))
        if op == "or":
            return _truth(evaluate(node[2], get)) or _truth(evaluate(node[3], get))
        a, b = evaluate(node[2], get), evaluate(node[3], get)
        if op in ("==", "!=", ">", ">=", "<", "<="):
            return _cmp(op, a, b)
        return _arith(op, a, b)
    if k == "call":
        name, args = node[1], node[2]
        if name == "if":
            return evaluate(args[1] if _truth(evaluate(args[0], get)) else args[2], get)
        if name == "ifs":
            for i in range(len(args) // 2):
                if _truth(evaluate(args[2 * i], get)):
                    return evaluate(args[2 * i + 1], get)
            return evaluate(args[-1], get) if len(args) % 2 else None
        if name == "and":
            return all(_truth(evaluate(a, get)) for a in args)
        if name == "or":
            return any(_truth(evaluate(a, get)) for a in args)
        return FUNCS[name][0](*[evaluate(a, get) for a in args])
    raise FormulaError(str(k))


def run(node, get):
    """算一格：出错（除以 0、类型不对）是空的。返回存的样子：数、文字、true/false、日期（{start}）、文字的列表。"""
    try:
        v = _clean(evaluate(node, get))
    except (FormulaError, TypeError, ValueError, OverflowError, ZeroDivisionError, AttributeError, IndexError, RecursionError):
        return None
    if isinstance(v, datetime):
        return {"start": text(v)}
    if isinstance(v, list):
        return [text(x) for x in v][:200] or None
    if v == "":
        return None
    return v


# ---------- 汇总 ----------
# 算法的名字（中文原文，显示时 i18n.t）
ROLLUPS = {"show": N_("显示原值"), "show_unique": N_("去重显示"), "count": N_("条数"), "count_values": N_("有值的个数"), "count_unique": N_("不同值的个数"),
           "count_empty": N_("空的个数"), "percent_empty": N_("空的占比"), "sum": N_("求和"), "average": N_("平均"), "median": N_("中位数"),
           "min": N_("最小"), "max": N_("最大"), "range": N_("最大减最小"), "earliest": N_("最早"), "latest": N_("最晚"),
           "checked": N_("勾了几个"), "percent_checked": N_("勾了的占比")}


def rollup(fn: str, values: list, total: int):
    """values 是对方每一行那个属性的值（展开成“给人看的”或数、日期），total 是关联了几行。"""
    flat = _flat(values)
    nums = [n for n in (num(v) for v in flat if not isinstance(v, (dict, datetime))) if n is not None]
    dates = sorted(d for d in (to_dt(v) for v in flat) if d is not None) if fn in ("earliest", "latest", "range") else []
    if fn == "show":
        return [text(v) for v in flat if not _empty(v)] or None
    if fn == "show_unique":
        return list(dict.fromkeys(text(v) for v in flat if not _empty(v))) or None
    if fn == "count":
        return total
    if fn == "count_values":
        return len([v for v in flat if not _empty(v)])
    if fn == "count_unique":
        return len({text(v) for v in flat if not _empty(v)})
    if fn == "count_empty":
        return total - len([v for v in values if not _empty(v)])
    if fn == "percent_empty":
        return round((total - len([v for v in values if not _empty(v)])) * 100 / total, 1) if total else None
    if fn == "checked":
        return len([v for v in values if v is True])
    if fn == "percent_checked":
        return round(len([v for v in values if v is True]) * 100 / total, 1) if total else None
    if fn in ("earliest", "latest", "range") and dates:
        if fn == "range":
            return round((dates[-1] - dates[0]).total_seconds() / 86400, 2)
        return {"start": text(dates[0] if fn == "earliest" else dates[-1])}
    if not nums:
        return None
    if fn == "sum":
        return _clean(float(sum(nums)))
    if fn == "average":
        return _clean(float(statistics.fmean(nums)))
    if fn == "median":
        return _clean(float(statistics.median(nums)))
    if fn == "min":
        return min(nums)
    if fn == "max":
        return max(nums)
    if fn == "range":
        return _clean(float(max(nums) - min(nums)))
    return None
