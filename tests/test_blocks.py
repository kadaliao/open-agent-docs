"""页面的切段（blocks.py）：每条规则、gap、对齐，以及切段的几条不变式。"""
from pathlib import Path

import pytest
from open_agent_docs import blocks as B

T, L = B.TIGHT, B.LOOSE


def parts(md):
    return B.split(md)


def mds(md):
    return [m for m, _ in B.split(md)]


@pytest.mark.parametrize("md, want", [
    # 列表：每个顶层项目一段，嵌套子项和续行跟着上一级
    ("- a\n- b\n  - 子项\n  续行\n- [ ] 待办", [("- a", ""), ("- b\n  - 子项\n  续行", T), ("- [ ] 待办", T)]),
    ("- a\n\n- b", [("- a", ""), ("- b", L)]),
    ("- a\n\n  第二段\n- b", [("- a\n\n  第二段", ""), ("- b", T)]),
    ("- a\n懒续行\n\n正文", [("- a\n懒续行", ""), ("正文", L)]),
    ("- a\n* b", [("- a", ""), ("* b", T)]),
    ("1. a\n2. b\n3) c", [("1. a", ""), ("2. b", T), ("3) c", T)]),
    ("- [x] 做完\n- 普通", [("- [x] 做完", ""), ("- 普通", T)]),
    # 表格：表头 + 分隔行一段，之后每行一段
    ("说明\n| 甲 | 乙 |\n| --- | :-: |\n| 1 | 2 |\n| 3 | 4 |\n\n后面",
     [("说明", ""), ("| 甲 | 乙 |\n| --- | :-: |", T), ("| 1 | 2 |", T), ("| 3 | 4 |", T), ("后面", L)]),
    ("a | b\n---", [("a | b\n---", "")]),   # 列数对不上：是标题，不是表格
    # 引用 / 提示框整段
    ("> [!NOTE]\n> 提示\n>\n> 第二段\n# 标题", [("> [!NOTE]\n> 提示\n>\n> 第二段", ""), ("# 标题", T)]),
    ("> 引用\n懒续行", [("> 引用\n懒续行", "")]),
    # 折叠块：到配对的 </details>，里面有空行、嵌套、围栏里的 </details> 都不算结束
    ("<details><summary>看</summary>\n\n正文\n\n<details>\n里面\n</details>\n\n```\n</details>\n```\n</details>\n后面",
     [("<details><summary>看</summary>\n\n正文\n\n<details>\n里面\n</details>\n\n```\n</details>\n```\n</details>", ""), ("后面", T)]),
    ("<details>\n没闭合\n\n后面", [("<details>\n没闭合", ""), ("后面", L)]),
    # 代码
    ("```js\na\n\nb\n```\n~~~\n```\n~~~", [("```js\na\n\nb\n```", ""), ("~~~\n```\n~~~", T)]),
    ("    代码1\n\n    代码2\n\n正文", [("    代码1\n\n    代码2", ""), ("正文", L)]),
    # 标题、分割线、setext
    ("标题\n---\n\n* * *\n- - -\n正文\n===", [("标题\n---", ""), ("* * *", L), ("- - -", T), ("正文\n===", T)]),
    ("# 一\n正文", [("# 一", ""), ("正文", T)]),
    # 段落：能打断的才断；2. 打断不了段落
    ("一行\n二行\n# 标题", [("一行\n二行", ""), ("# 标题", T)]),
    ("一行\n2. 不是列表", [("一行\n2. 不是列表", "")]),
    ("一行\n- 列表", [("一行", ""), ("- 列表", T)]),
    # HTML 块
    ("<div>\nx\n</div>\n\n<!-- 注释\n-->\n<pre>\n\nq\n</pre>", [("<div>\nx\n</div>", ""), ("<!-- 注释\n-->", L), ("<pre>\n\nq\n</pre>", T)]),
    ("<!-- 一行 -->\n正文", [("<!-- 一行 -->", ""), ("正文", T)]),
    # 换行和空行
    ("a\r\nb\r\n\r\n\r\n\r\nc\n\n", [("a\nb", ""), ("c", L)]),
    ("", []),
    ("\n\n  \n", []),
])
def test_split(md, want):
    assert parts(md) == want


FIXTURES = sorted((Path(__file__).parent / "fixtures" / "md_blocks").glob("*.md"))
EXTRA = ["- a\n- b\n\n- c\n1. d", "| a |\n| - |\n| 1 |\n正文", "> x\n>\n- y", "```\n没闭合", "<details>\n<details>\n</details>"]


@pytest.mark.parametrize("md", [f.read_text() for f in FIXTURES] + EXTRA)
def test_invariants(md):
    ps = B.split(md)
    lines = B.normalize(md).split("\n")
    # 每段都是原文里连续的几行（一个字符都没改）
    for m, _ in ps:
        n = m.count("\n") + 1
        assert any(lines[i:i + n] == m.split("\n") for i in range(len(lines) - n + 1)), m
    # 单独切一段还是它自己；拼回去再切不变
    assert all(B.split(m) == [(m, "")] for m, _ in ps)
    assert B.split(B.join(ps)) == ps


def test_fixtures_exist():
    assert len(FIXTURES) >= 2


def test_fix_gaps():
    # 表格行接在表格后面紧挨着；同一种列表项沿用原来的松紧；其余隔一个空行
    m = ["| a | b |\n| - | - |", "| 1 | 2 |", "| 3 | 4 |"]
    assert B.fix_gaps(m, ["", T, L], {2}) == ["", T, T]
    m = ["- a", "- b", "- c"]
    assert B.fix_gaps(m, ["", L, T], {2}) == ["", L, L]
    assert B.fix_gaps(m, ["", T, L], {2}) == ["", T, T]
    assert B.fix_gaps(["- a", "1. b"], ["", T], {1}) == ["", L]
    assert B.fix_gaps(["正文", "- a"], ["", T], {1}) == ["", L]
    assert B.fix_gaps(["正文", "| 1 | 2 |"], ["", T], {1}) == ["", L]   # 前面不是表格：管道符只是普通文字
    assert B.fix_gaps(["a", "b"], ["x", T], set()) == ["", T]   # 第一段永远没有 gap；不在 dirty 里的不动


def test_kinds():
    assert B.list_kind("- a") == "-" and B.list_kind("* a") == "*" and B.list_kind("3) a") == "ord)"
    assert B.list_kind("* * *") is None and B.list_kind("  - a") is None
    assert B.kind("| a | b |\n| - | - |") == "thead" and B.kind("| 1 | 2 |") == "pipe" and B.kind("# x") == "other"


def test_ids():
    taken = set()
    a = B.new_id(taken)
    assert len(a) == 4 and a in taken and B.valid_id(a) and B.new_id(taken) != a
    assert not B.valid_id("x") and not B.valid_id("a" * 17) and not B.valid_id(None)
    assert B.clean_id("⟦k7qd⟧") == B.clean_id("#k7qd") == B.clean_id(" k7qd ") == "k7qd"


def test_strip_ids():
    assert B.strip_ids("⟦k7qd⟧\n# 标题\n\n⟦m2xa⟧\n正文") == "# 标题\n\n正文"
    assert B.strip_ids("⟦k7qd⟧ # 标题") == "# 标题"
    assert B.strip_ids("[abcd]\n普通的方括号") == "[abcd]\n普通的方括号"


def test_align():
    old = ["a", "b", "c", "d"]
    assert B.align(old, ["a", "B", "c", "d"]) == [(0, True), (1, False), (2, True), (3, True)]
    assert B.align(old, ["x", "a", "b", "c", "d"]) == [(None, False), (0, True), (1, True), (2, True), (3, True)]
    assert B.align(old, ["a", "d"]) == [(0, True), (3, True)]


def test_groups():
    work = [("段一", ""), ("续行", T)]
    canon = B.split(B.join(work))
    assert canon == [("段一\n续行", "")]
    assert B.groups(work, canon) == [[0, 1]]
    same = [("# 一", ""), ("正文", L)]
    assert B.groups(same, B.split(B.join(same))) == [[0], [1]]
