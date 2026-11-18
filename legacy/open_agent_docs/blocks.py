"""资料库页面的“段”：把 Markdown 正文切成一段一段。Bot 按段改内容、用户按段评论、界面按段标出是谁写的。

切法（服务器是唯一标准，网页编辑器保存后按这里切出来的结果校正）：
- 只切行区间，不改段里的文字：Bot 写的原文一个字符都不动。写入时只做三件事：换行统一成 \\n、去掉段首段尾的空行、
  段与段之间多个空行合成一个。
- 列表的每个顶层项目（含待办）是一段，表格的表头（连同分隔行）是一段、之后每一行是一段；引用、提示框（> [!NOTE]）、
  代码块、折叠块（<details>）、标题、段落、分割线各是一段。嵌套的子项跟着上一级项目。
- 每段记着和上一段之间隔的是什么（gap）：紧挨着（\\n，同一列表里的紧凑项目、表格的行）还是隔一个空行（\\n\\n）。
  gap 从正文算出来，不另存；join 按 gap 拼回去，split(join(parts)) == parts。

规则照着 CommonMark / GFM 的块结构写，够切段用就行，不追求和渲染器逐条一致：切出来的边界确定、可重复才是要紧的。
不管 $$ 公式、front matter、跨段的引用式链接定义、脚注。
"""
import difflib
import hashlib
import re
import secrets

TIGHT, LOOSE = "\n", "\n\n"
_ID = re.compile(r"[A-Za-z0-9]{4,16}")
_ID_LINE = re.compile(r"^[ \t]*⟦#?[A-Za-z0-9]{4,16}⟧[ \t]*$")   # 模型把段落编号抄进正文
_ID_HEAD = re.compile(r"^([ \t]*)⟦#?[A-Za-z0-9]{4,16}⟧[ \t]?")

_FENCE = re.compile(r"^( {0,3})(`{3,}|~{3,})(.*)$")
_DETAILS_START = re.compile(r"(?i)^ {0,3}<details(?=[\s>/]|$)")
_DETAILS_OPEN = re.compile(r"(?i)<details(?=[\s>/]|$)")
_DETAILS_CLOSE = re.compile(r"(?i)</details\s*>")
_ATX = re.compile(r"^ {0,3}#{1,6}(?:[ \t]|$)")
_HR = re.compile(r"^ {0,3}(?:(?:\*[ \t]*){3,}|(?:-[ \t]*){3,}|(?:_[ \t]*){3,})$")
_SETEXT = re.compile(r"^ {0,3}(?:=+|-+)[ \t]*$")
_QUOTE = re.compile(r"^ {0,3}>")
_QUOTE_EMPTY = re.compile(r"^ {0,3}>[ \t]*$")
_ITEM = re.compile(r"^( {0,3})([-+*]|\d{1,9}[.)])(?=[ \t]|$)")
_HTML1 = re.compile(r"(?i)^ {0,3}<(?:script|pre|style|textarea)(?:[\s>]|$)")
_HTML1_END = re.compile(r"(?i)</(?:script|pre|style|textarea)>")
_HTML2 = re.compile(r"^ {0,3}<!--")
_HTML6 = re.compile(
    r"(?i)^ {0,3}</?(?:address|article|aside|base|basefont|blockquote|body|caption|center|col|colgroup|dd|details|dialog|"
    r"dir|div|dl|dt|fieldset|figcaption|figure|footer|form|frame|frameset|h[1-6]|head|header|hr|html|iframe|legend|li|"
    r"link|main|menu|menuitem|nav|noframes|ol|optgroup|option|p|param|search|section|summary|table|tbody|td|tfoot|th|"
    r"thead|title|tr|track|ul)(?:\s|/?>|$)")
_HTML7 = re.compile(r"""^ {0,3}(?:<[A-Za-z][A-Za-z0-9-]*(?:\s+[A-Za-z_:][\w.:-]*(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s"'=<>`]+))?)*\s*/?>"""
                    r"|</[A-Za-z][A-Za-z0-9-]*\s*>)\s*$")
_DELIM = re.compile(r"^ {0,3}\|?[ \t]*:?-+:?[ \t]*(?:\|[ \t]*:?-+:?[ \t]*)*\|?[ \t]*$")


# ---------- 小工具 ----------
def _blank(line: str) -> bool:
    return not line.strip()


def _indent(line: str) -> int:
    """行首缩进的列数（tab 按 4 列对齐）。"""
    n = 0
    for ch in line:
        if ch == " ":
            n += 1
        elif ch == "\t":
            n += 4 - n % 4
        else:
            break
    return n


def _cells(line: str) -> int:
    s = line.strip()
    s = s[1:] if s.startswith("|") else s
    s = s[:-1] if s.endswith("|") and not s.endswith("\\|") else s
    return len(re.split(r"(?<!\\)\|", s))


def _table_head(lines: list[str], i: int) -> bool:
    return (i + 1 < len(lines) and "|" in lines[i] and _indent(lines[i]) < 4 and "-" in lines[i + 1]
            and bool(_DELIM.match(lines[i + 1])) and _cells(lines[i]) == _cells(lines[i + 1]))


def _fence(line: str):
    """围栏代码的开头：返回 (字符, 长度)，不是就 None。反引号围栏的信息串里不能再有反引号。"""
    m = _FENCE.match(line)
    if not m or (m[2][0] == "`" and "`" in m[3]):
        return None
    return m[2][0], len(m[2])


def _closes(line: str, fence) -> bool:
    ch, n = fence
    return bool(re.match(rf"^ {{0,3}}{re.escape(ch)}{{{n},}}[ \t]*$", line))


def _item(line: str):
    """列表项的开头：返回 (标记的缩进, 标记, 内容列, 后面有没有内容)。分割线（* * *）不算。"""
    m = _ITEM.match(line)
    if not m or _HR.match(line):
        return None
    ind, mk = _indent(m[1]), m[2]
    rest = line[m.end():]
    if _blank(rest):
        return ind, mk, ind + len(mk) + 1, False
    sp = _indent(rest)
    return ind, mk, ind + len(mk) + (sp if 1 <= sp <= 4 else 1), True


def _interrupts(lines: list[str], j: int) -> bool:
    """这一行能不能打断上面的段落（不能的话就是段落的续行）。"""
    line = lines[j]
    if _indent(line) >= 4:
        return False
    if _ATX.match(line) or _fence(line) or _QUOTE.match(line) or _HR.match(line):
        return True
    if _HTML1.match(line) or _HTML2.match(line) or _HTML6.match(line):
        return True
    if (it := _item(line)) and it[3]:
        mk = it[1]
        return mk in "-+*" or int(mk[:-1]) == 1
    return _table_head(lines, j)


# ---------- 各种段在哪里结束（返回结束行的下一行；可能带着段尾的空行，调用方会去掉） ----------
def _end_fence(lines, i):
    f = _fence(lines[i])
    for j in range(i + 1, len(lines)):
        if _closes(lines[j], f):
            return j + 1
    return len(lines)


def _end_details(lines, i):
    """<details> 到配对的 </details>：数嵌套层数，围栏代码里的不算。没闭合就返回 None。"""
    depth, fence = 0, None
    for j in range(i, len(lines)):
        line = lines[j]
        if fence:
            if _closes(line, fence):
                fence = None
            continue
        if j > i and (f := _fence(line)):
            fence = f
            continue
        depth += len(_DETAILS_OPEN.findall(line)) - len(_DETAILS_CLOSE.findall(line))
        if depth <= 0:
            return j + 1
    return None


def _end_until(lines, i, opener, closer):
    """HTML 块（<pre>、<script>、<!-- 这类）到结束标记那一行为止；结束标记可以和开头在同一行。"""
    if closer.search(lines[i], opener.match(lines[i]).end()):
        return i + 1
    for j in range(i + 1, len(lines)):
        if closer.search(lines[j]):
            return j + 1
    return len(lines)


def _end_blank(lines, i):
    j = i + 1
    while j < len(lines) and not _blank(lines[j]):
        j += 1
    return j


def _end_quote(lines, i):
    j = i + 1
    while j < len(lines):
        line = lines[j]
        if _blank(line):
            break
        if _QUOTE.match(line):
            j += 1
            continue
        if _QUOTE_EMPTY.match(lines[j - 1]) or _interrupts(lines, j):   # 懒续行只接在引用里的段落后面
            break
        j += 1
    return j


def _end_item(lines, i):
    it = _item(lines[i])
    width = it[2] if it else 0
    j = i + 1
    while j < len(lines):
        line = lines[j]
        if _blank(line):
            k = j
            while k < len(lines) and _blank(lines[k]):
                k += 1
            if k < len(lines) and _indent(lines[k]) >= width:
                j = k
                continue
            break
        if _indent(line) >= width:
            j += 1
            continue
        if _item(line) or _interrupts(lines, j):   # 同级的下一项、新的列表，或者能打断段落的东西
            break
        j += 1   # 懒续行
    return j


def _end_code(lines, i):
    last, j = i + 1, i + 1
    while j < len(lines):
        if _blank(lines[j]):
            j += 1
        elif _indent(lines[j]) >= 4:
            j += 1
            last = j
        else:
            break
    return last


def _end_para(lines, i):
    j = i + 1
    while j < len(lines):
        line = lines[j]
        if _blank(line):
            break
        if _SETEXT.match(line):   # 段落下面的 === / --- 是标题的下划线，不是分割线
            return j + 1
        if _interrupts(lines, j):
            break
        j += 1
    return j


_HTML2_END = re.compile("-->")


def _end(lines: list[str], i: int) -> int | None:
    """从第 i 行开始的段在哪里结束。是段落（包括可能的表格）就返回 None，由 split 处理。"""
    line = lines[i]
    if _indent(line) >= 4:
        return _end_code(lines, i)
    if _fence(line):
        return _end_fence(lines, i)
    if _DETAILS_START.match(line) and (j := _end_details(lines, i)):
        return j
    if _HTML1.match(line):
        return _end_until(lines, i, _HTML1, _HTML1_END)
    if _HTML2.match(line):
        return _end_until(lines, i, _HTML2, _HTML2_END)
    if _HTML6.match(line) or _HTML7.match(line):
        return _end_blank(lines, i)
    if _ATX.match(line) or _HR.match(line):
        return i + 1
    if _QUOTE.match(line):
        return _end_quote(lines, i)
    if _item(line):
        return _end_item(lines, i)
    return None


def _table_breaks(line: str) -> bool:
    return (_blank(line) or _indent(line) >= 4 or bool(_ATX.match(line) or _fence(line) or _QUOTE.match(line)
            or _HR.match(line) or _item(line) or _HTML1.match(line) or _HTML2.match(line) or _HTML6.match(line)))


# ---------- 切开、拼回 ----------
def normalize(md: str) -> str:
    return (md or "").replace("\r\n", "\n").replace("\r", "\n")


def split(md: str) -> list[tuple[str, str]]:
    """正文 → [(段的原文, 和上一段之间的 gap)]。第一段的 gap 是空串。"""
    lines = normalize(md).split("\n")
    out: list[tuple[str, str]] = []
    i, n, blank = 0, len(lines), False

    def add(a: int, b: int):
        nonlocal blank
        k = b
        while k > a and _blank(lines[k - 1]):
            k -= 1
        out.append(("\n".join(lines[a:k]), (LOOSE if blank else TIGHT) if out else ""))
        blank = k < b

    while i < n:
        if _blank(lines[i]):
            blank, i = True, i + 1
            continue
        j = _end(lines, i)
        if j is None and _table_head(lines, i):   # 表格：表头 + 分隔行一段，之后每行一段
            add(i, i + 2)
            j = i + 2
            while j < n and not _table_breaks(lines[j]):
                add(j, j + 1)
                j += 1
        else:
            j = j or _end_para(lines, i)
            add(i, j)
        i = j
    return out


def join(parts: list[tuple[str, str]]) -> str:
    text = ""
    for k, (md, gap) in enumerate(parts):
        text = md if k == 0 else text + (gap if gap in (TIGHT, LOOSE) else LOOSE) + md
    return text


# ---------- 段的种类和 gap ----------
def _first(md: str) -> str:
    return md.split("\n", 1)[0]


def list_kind(md: str) -> str | None:
    """列表项的种类：无序看标记字符，有序看分隔符（. 或 )）。同一种的相邻项才在同一个列表里。"""
    it = _item(_first(md))
    if not it or it[0] > 0:
        return None
    mk = it[1]
    return mk if mk in "-+*" else "ord" + mk[-1]


def is_head(md: str) -> bool:
    return _table_head(md.split("\n")[:2], 0)


def kind(md: str) -> str:
    """粗分类，用来判断改完以后 gap 要不要重新算。"""
    if k := list_kind(md):
        return "item" + k
    if is_head(md):
        return "thead"
    if "\n" not in md and "|" in md:
        return "pipe"
    return "other"


def fix_gaps(mds: list[str], gaps: list[str], dirty: set[int]) -> list[str]:
    """重新算 dirty 里那些位置的 gap（其余照旧）：表格行接在表格后面、同一种列表项接在一起时紧挨着，
    列表沿用原来的松紧；其余隔一个空行。从前往后算，后面的能用到前面刚算好的。"""
    gaps = list(gaps)
    for i in range(len(mds)):
        if i == 0:
            gaps[0] = ""
            continue
        if i not in dirty:
            continue
        prev, cur = mds[i - 1], mds[i]
        if kind(cur) == "pipe" and _in_table(mds, gaps, i - 1):
            gaps[i] = TIGHT
        elif (k := list_kind(cur)) and list_kind(prev) == k:
            if i >= 2 and list_kind(mds[i - 2]) == k and i - 1 not in dirty:
                gaps[i] = gaps[i - 1]
            elif i + 1 < len(mds) and list_kind(mds[i + 1]) == k and i + 1 not in dirty:
                gaps[i] = gaps[i + 1]
            else:
                gaps[i] = TIGHT
        else:
            gaps[i] = LOOSE
    return gaps


def _in_table(mds, gaps, i) -> bool:
    while i >= 0:
        if is_head(mds[i]):
            return True
        if kind(mds[i]) != "pipe" or gaps[i] != TIGHT:
            return False
        i -= 1
    return False


# ---------- 编号、指纹、对齐 ----------
def digest(md: str) -> str:
    return hashlib.sha1(md.encode()).hexdigest()[:10]


def new_id(taken: set) -> str:
    while True:
        i = "".join(secrets.choice("abcdefghijklmnopqrstuvwxyz0123456789") for _ in range(4))
        if i not in taken and not i.isdigit():
            taken.add(i)
            return i


def valid_id(i) -> bool:
    return isinstance(i, str) and bool(_ID.fullmatch(i))


def clean_id(i) -> str:
    """模型给的段落编号可能写成 ⟦k7qd⟧、#k7qd、[k7qd]。"""
    return str(i or "").strip().strip("⟦⟧[]#").strip()


def strip_ids(text: str) -> str:
    """去掉模型照抄进正文的段落编号（read_page 里每段前面那行 ⟦xxxx⟧）。"""
    lines = [ln for ln in normalize(text).split("\n") if not _ID_LINE.match(ln)]
    return "\n".join(_ID_HEAD.sub(r"\1", ln) for ln in lines)


def align(old: list, new: list) -> list[tuple[int | None, bool]]:
    """新的每一段对应旧的哪一段：(旧的下标或 None, 内容是不是没变)。内容相同的对上；被改的一组里按位置一对一，
    保留旧的编号（评论还挂在上面），算作改过。"""
    out: list[tuple[int | None, bool]] = [(None, False)] * len(new)
    for tag, i1, i2, j1, j2 in difflib.SequenceMatcher(None, old, new, autojunk=False).get_opcodes():
        if tag == "equal":
            for k in range(j2 - j1):
                out[j1 + k] = (i1 + k, True)
        elif tag == "replace":
            for k in range(min(i2 - i1, j2 - j1)):
                out[j1 + k] = (i1 + k, False)
    return out


def groups(work: list[tuple[str, str]], canon: list[tuple[str, str]]) -> list[list[int]] | None:
    """拼起来再切一遍以后，新的每一段由原来的哪几段组成（拼接只会让相邻的段合并）。
    原来的某一段被切成了好几段（不该发生）就返回 None，调用方改用 align。"""
    text = join(work)
    spans, pos = [], 0
    for k, (md, gap) in enumerate(work):
        pos += len(gap if gap in (TIGHT, LOOSE) else LOOSE) if k else 0
        spans.append((pos, pos + len(md)))
        pos += len(md)
    out, cur = [], 0
    for md, _ in canon:
        at = text.find(md, cur)
        if at < 0:
            return None
        end = at + len(md)
        g = [k for k, (a, b) in enumerate(spans) if a < end and b > at]
        if not g or any(spans[k][0] < at or spans[k][1] > end for k in g):
            return None
        out.append(g)
        cur = end
    return out if sorted(k for g in out for k in g) == list(range(len(work))) else None
