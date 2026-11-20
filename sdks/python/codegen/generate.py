#!/usr/bin/env python3
"""读 docs/openapi.json 和 sdks/operations.json，生成 Python SDK 的模型和资源类。

    python sdks/python/codegen/generate.py           # 写 src/open_agent_docs/_generated/
    python sdks/python/codegen/generate.py --check   # 只比较：和仓库里的不一样就退出 1

只用标准库，能反复跑：openapi.json 变了（加了字段、接口、事件）重新跑一遍就行，手写的运行时代码不用动。
名字和返回值的整理（unwrap / paginate / null_on_404 / sse / upload / download）看 operations.json，
规则见 docs/sdk-contract.md 的 §2、§6。
"""
from __future__ import annotations

import argparse
import difflib
import json
import keyword
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

ROOT = Path(__file__).resolve().parents[3]
SPEC_PATH = ROOT / "docs" / "openapi.json"
OPS_PATH = ROOT / "sdks" / "operations.json"
OUT_DIR = ROOT / "sdks" / "python" / "src" / "open_agent_docs" / "_generated"

HEADER = "# 由 codegen 生成，不要手改：python sdks/python/codegen/generate.py（来源 docs/openapi.json、sdks/operations.json）\n"
RESOURCES_PRAGMA = "# pyright: reportPrivateUsage=false\n"

# 运行时在包顶层导出的名字：模型不能和它们重名
RUNTIME_NAMES = {
    "Client", "AsyncClient", "Event", "EventStream", "AsyncEventStream", "Server", "CursorPage",
    "AsyncCursorPage", "AsyncPaginator", "NotGiven", "NOT_GIVEN", "OADError", "APIError",
    "ConfigurationError", "WebhookVerificationError", "APIConnectionError", "APITimeoutError",
    "BaseModel", "start", "sign_jwt", "embed_url", "verify_webhook",
}
# 每个方法都有的选项参数：接口的参数不能和它们重名
OPTION_PARAMS = ["extra_headers", "extra_query", "extra_body", "timeout"]
# pydantic.BaseModel 上已有的属性：字段可以同名（警告压掉、类型检查忽略），访问时拿到的是字段
BASEMODEL_ATTRS = {"schema", "json", "dict", "copy", "construct", "validate", "fields", "parse_obj", "parse_raw", "from_orm"}


def die(msg: str) -> None:
    print("✗ " + msg, file=sys.stderr)
    sys.exit(2)


# ---------------------------------------------------------------- 类型

class Spec:
    def __init__(self, spec: Dict[str, Any]):
        self.spec = spec
        self.schemas: Dict[str, Any] = spec["components"]["schemas"]

    def ref_name(self, s: Dict[str, Any]) -> str:
        return s["$ref"].rsplit("/", 1)[-1]

    def deref(self, s: Any) -> Any:
        while isinstance(s, dict) and "$ref" in s:
            s = self.schemas[self.ref_name(s)]
        return s

    def is_model(self, name: str) -> bool:
        """这个组件生成的是类（不是类型别名）"""
        s = self.schemas[name]
        if "allOf" in s:
            return True
        return s.get("type") == "object" and bool(s.get("properties"))


def is_null(s: Any) -> bool:
    return isinstance(s, dict) and (s.get("type") == "null" or s.get("type") == ["null"])


def uniq(xs: List[str]) -> List[str]:
    out: List[str] = []
    for x in xs:
        if x not in out:
            out.append(x)
    return out


def literal(values: List[Any]) -> str:
    return "Literal[" + ", ".join(json.dumps(v, ensure_ascii=False) for v in values) + "]"


def py_type(sp: Spec, s: Any, inp: bool = False) -> Tuple[str, bool]:
    """schema → (Python 类型, 可不可以是 null)。inp=True 是请求参数：模型也收 dict，列表收 Sequence。"""
    if not isinstance(s, dict) or not s:
        return "Any", False
    if "$ref" in s:
        name = sp.ref_name(s)
        if inp and sp.is_model(name):
            return f"Union[{name}, Dict[str, Any]]", False
        return name, False
    for k in ("oneOf", "anyOf"):
        if k in s:
            members = s[k]
            nullable = any(is_null(m) for m in members)
            parts: List[str] = []
            for m in members:
                if is_null(m):
                    continue
                t, n = py_type(sp, m, inp)
                nullable = nullable or n
                parts.append(t)
            parts = uniq(parts)
            if not parts:
                return "None", True
            return (parts[0] if len(parts) == 1 else "Union[" + ", ".join(parts) + "]"), nullable
    if "allOf" in s:
        if len(s["allOf"]) == 1:
            return py_type(sp, s["allOf"][0], inp)
        return "Dict[str, Any]", False
    t = s.get("type")
    nullable = False
    if isinstance(t, list):
        nullable = "null" in t
        rest = [x for x in t if x != "null"]
        t = rest[0] if len(rest) == 1 else None
    if "enum" in s:
        vals = [v for v in s["enum"] if v is not None]
        if t == "string" or all(isinstance(v, str) for v in vals):
            # 宽松：服务器加了新值也不报错
            return f"Union[{literal(vals)}, str]", nullable or None in s["enum"]
        return literal(vals), nullable
    if t == "string":
        return "str", nullable
    if t == "integer":
        return "int", nullable
    if t == "number":
        return "float", nullable
    if t == "boolean":
        return "bool", nullable
    if t == "null":
        return "None", True
    if t == "array":
        it, n = py_type(sp, s.get("items"), inp)
        if n:
            it = f"Optional[{it}]"
        return (f"Sequence[{it}]" if inp else f"List[{it}]"), nullable
    if t == "object":
        ap = s.get("additionalProperties")
        if isinstance(ap, dict) and ap:
            vt, n = py_type(sp, ap, inp)
            if n:
                vt = f"Optional[{vt}]"
            return (f"Mapping[str, {vt}]" if inp else f"Dict[str, {vt}]"), nullable
        return ("Mapping[str, Any]" if inp else "Dict[str, Any]"), nullable
    return "Any", nullable


def optional(t: str) -> str:
    if t == "Any" or t == "None":
        return t
    return f"Optional[{t}]"


def ident(name: str) -> str:
    """接口里的名字 → Python 标识符"""
    n = re.sub(r"[^0-9a-zA-Z_]", "_", name)
    if n[:1].isdigit():
        n = "_" + n
    if keyword.iskeyword(n) or n in ("self", "cls"):
        n += "_"
    return n


def pascal(name: str) -> str:
    return "".join(p[:1].upper() + p[1:] for p in re.split(r"[^0-9a-zA-Z]+", name) if p)


def doc_lines(text: Optional[str], indent: str) -> List[str]:
    """一段说明 → 三引号 docstring 的几行"""
    if not text:
        return []
    text = text.strip().replace("\\", "\\\\").replace('"""', '\\"\\"\\"')
    lines = text.split("\n")
    if len(lines) == 1 and not text.endswith('"'):
        return [f'{indent}"""{text}"""']
    return [f'{indent}"""{lines[0]}'] + [(indent + ln).rstrip() for ln in lines[1:]] + [f'{indent}"""']


# ---------------------------------------------------------------- 模型

@dataclass
class ModelOut:
    name: str
    base: str
    lines: List[str]
    deps: List[str]


def gen_fields(sp: Spec, s: Dict[str, Any], owner: str) -> Tuple[List[str], List[str]]:
    """对象的属性 → 类里的几行（和用到的组件名）"""
    req = set(s.get("required", []))
    out: List[str] = []
    deps: List[str] = []
    for pname, ps in (s.get("properties") or {}).items():
        t, nullable = py_type(sp, ps)
        deps.extend(re.findall(r"\b[A-Z][A-Za-z0-9]*\b", t))
        attr = ident(pname)
        alias = f'alias="{pname}"' if attr != pname else ""
        ignore = ""
        if attr in BASEMODEL_ATTRS:
            ignore = "  # pyright: ignore[reportIncompatibleMethodOverride, reportIncompatibleVariableOverride]"
        if pname in req and not nullable:
            line = f"    {attr}: {t}" + (f" = Field({alias})" if alias else "")
        else:
            default = f"Field(default=None, {alias})" if alias else "None"
            line = f"    {attr}: {optional(t)} = {default}"
        out.append(line + ignore)
        d = ps.get("description") if isinstance(ps, dict) else None
        out.extend(doc_lines(d, "    "))
    return out, deps


def gen_models(sp: Spec) -> str:
    classes: Dict[str, ModelOut] = {}
    aliases: Dict[str, Tuple[str, List[str], Optional[str]]] = {}
    for name in sorted(sp.schemas):
        if name in RUNTIME_NAMES:
            die(f"模型 {name} 和运行时导出的名字重名")
        s = sp.schemas[name]
        desc = s.get("description")
        if "allOf" in s:
            base = "BaseModel"
            lines: List[str] = []
            deps: List[str] = []
            for part in s["allOf"]:
                if "$ref" in part:
                    if base != "BaseModel":
                        die(f"{name}：allOf 里有两个 $ref，生成器还不会")
                    base = sp.ref_name(part)
                    deps.append(base)
                else:
                    fl, fd = gen_fields(sp, part, name)
                    lines += fl
                    deps += fd
            classes[name] = ModelOut(name, base, lines, deps)
            classes[name].lines = doc_lines(desc, "    ") + lines
        elif sp.is_model(name):
            fl, fd = gen_fields(sp, s, name)
            classes[name] = ModelOut(name, "BaseModel", doc_lines(desc, "    ") + fl, fd)
        else:
            t, nullable = py_type(sp, s)
            if nullable:
                t = optional(t)
            aliases[name] = (t, re.findall(r"\b[A-Z][A-Za-z0-9]*\b", t), desc)

    # 事件：openapi 的 webhooks（事件名 → 正文的 schema）。直接写的对象生成 <事件名>Event 类
    events: List[Tuple[str, str]] = []
    for ev, item in sorted(sp.spec.get("webhooks", {}).items()):
        schema = item.get("post", {}).get("requestBody", {}).get("content", {}).get("application/json", {}).get("schema")
        if not schema:
            continue
        if "$ref" in schema:
            n = sp.ref_name(schema)
            if sp.is_model(n):
                events.append((ev, n))
        elif schema.get("properties"):
            n = pascal(ev) + "Event"
            if n in sp.schemas or n in classes:
                die(f"事件模型 {n} 和组件重名")
            fl, fd = gen_fields(sp, schema, n)
            classes[n] = ModelOut(n, "BaseModel", doc_lines(f"SSE 事件 `{ev}` 的 data。", "    ") + fl, fd)
            events.append((ev, n))

    # 类按依赖排（父类在前），别的按名字
    ordered: List[str] = []
    seen: set[str] = set()

    def visit(n: str) -> None:
        if n in seen:
            return
        seen.add(n)
        b = classes[n].base
        if b in classes:
            visit(b)
        ordered.append(n)

    for n in sorted(classes):
        visit(n)

    # 类型别名放在类后面，互相之间按依赖排
    alias_order: List[str] = []
    aseen: set[str] = set()

    def avisit(n: str) -> None:
        if n in aseen:
            return
        aseen.add(n)
        for d in aliases[n][1]:
            if d in aliases:
                avisit(d)
        alias_order.append(n)

    for n in sorted(aliases):
        avisit(n)

    names = sorted(list(classes) + list(aliases))
    out: List[str] = [HEADER.rstrip("\n"), '"""接口的模型（docs/openapi.json 的 components.schemas 和 webhooks）。"""', "from __future__ import annotations", ""]
    out += ["import warnings", "from typing import Any, Dict, List, Literal, Optional, Type, Union", "", "from pydantic import Field", "", "from .._models import BaseModel", ""]
    out.append("__all__ = [")
    for n in names:
        out.append(f'    "{n}",')
    out.append("]")
    out.append("")
    out.append("# 有的字段叫 schema、json，和 pydantic.BaseModel 的老方法同名：照样能用（拿到的是字段），警告压掉")
    out.append('warnings.filterwarnings("ignore", message=r"Field name \\"(' + "|".join(sorted(BASEMODEL_ATTRS)) + ')\\" .* shadows an attribute")')
    for n in ordered:
        c = classes[n]
        out += ["", "", f"class {n}({c.base}):"]
        out += c.lines if c.lines else ["    pass"]
    out.append("")
    for n in alias_order:
        t, _, desc = aliases[n]
        out += ["", f"{n} = {t}"]
        out += doc_lines(desc, "")
    out += ["", "", "# 工作区事件流里，事件名 → data 的模型（没列的给原始 JSON）", "EVENT_MODELS: Dict[str, Type[BaseModel]] = {"]
    for ev, n in events:
        out.append(f'    "{ev}": {n},')
    out += ["}", ""]
    out += ["# 互相引用的、引用后面类型别名的：都定义完了再解析一遍", "for _m in ("]
    out += [f"    {n}," for n in ordered]
    out += ["):", "    _m.model_rebuild()", "del _m", ""]
    return "\n".join(out)


# ---------------------------------------------------------------- 接口

@dataclass
class Param:
    name: str  # 接口里的名字
    py: str  # Python 参数名
    type: str
    required: bool
    nullable: bool = False
    desc: Optional[str] = None


@dataclass
class Op:
    id: str
    name: List[str]
    method: str
    path: str
    hints: Dict[str, Any]
    summary: Optional[str]
    path_params: List[Param] = field(default_factory=list)
    query: List[Param] = field(default_factory=list)
    body_kind: Optional[str] = None  # fields / raw / None
    body_fields: List[Param] = field(default_factory=list)
    body_required: bool = False
    body_type: str = "Any"
    resp: str = "none"  # none / model / bytes / sse
    resp_model: Optional[str] = None


def build_ops(sp: Spec, table: Dict[str, Any]) -> List[Op]:
    by_id: Dict[str, Tuple[str, str, Dict[str, Any]]] = {}
    for path, item in sp.spec["paths"].items():
        for method, op in item.items():
            if method in ("get", "post", "put", "patch", "delete"):
                by_id[op["operationId"]] = (path, method, op)
    ops: List[Op] = []
    for oid, hints in table["operations"].items():
        if oid not in by_id:
            die(f"operations.json 的 {oid} 在 openapi.json 里没有（先跑 node sdks/check-operations.mjs）")
        path, method, raw = by_id[oid]
        o = Op(oid, hints["name"].split("."), method.upper(), path, hints, raw.get("description") or raw.get("summary"))
        order = re.findall(r"\{([^}]+)\}", path)
        params = {p["name"]: p for p in raw.get("parameters", [])}
        for pn in order:
            p = params.get(pn, {"schema": {"type": "string"}})
            t, _ = py_type(sp, p.get("schema"))
            o.path_params.append(Param(pn, ident(pn), t, True, desc=p.get("description")))
        for p in raw.get("parameters", []):
            if p["in"] != "query":
                continue  # 头（Range、Last-Event-ID、X-Filename）运行时自己处理
            t, n = py_type(sp, p.get("schema"))
            o.query.append(Param(p["name"], ident(p["name"]), t, bool(p.get("required")), n, p.get("description")))
        rb = raw.get("requestBody")
        if rb and not hints.get("upload"):
            schema = rb.get("content", {}).get("application/json", {}).get("schema")
            if schema is None:
                die(f"{oid}：请求体不是 JSON，又没有 upload 提示")
            members = [schema]
            nullable_body = False
            if "oneOf" in schema:
                nn = [m for m in schema["oneOf"] if not is_null(m)]
                nullable_body = len(nn) < len(schema["oneOf"])
                members = nn
            target = sp.deref(members[0]) if len(members) == 1 else None
            if target and target.get("type") == "object" and target.get("properties") and "allOf" not in target:
                o.body_kind = "fields"
                req = set(target.get("required", [])) if rb.get("required") and not nullable_body else set()
                for fname, fs in target["properties"].items():
                    t, n = py_type(sp, fs, inp=True)
                    o.body_fields.append(Param(fname, ident(fname), t, fname in req and not n, n, fs.get("description")))
            else:
                o.body_kind = "raw"
                o.body_required = bool(rb.get("required"))
                t, n = py_type(sp, schema, inp=True)
                o.body_type = optional(t) if n else t
        # 响应
        resp = raw.get("responses", {})
        ok = sorted(k for k in resp if k.startswith("2"))
        content = (resp[ok[0]].get("content") or {}) if ok else {}
        if hints.get("sse"):
            o.resp = "sse"
        elif hints.get("download"):
            o.resp = "bytes"
        elif "application/json" in content:
            s = content["application/json"].get("schema")
            if s and "$ref" in s:
                o.resp = "model"
                o.resp_model = sp.ref_name(s)
            else:
                o.resp = "json"
        elif content:
            o.resp = "bytes"
        else:
            o.resp = "none"
        # 名字检查
        names = [p.py for p in o.path_params] + [p.py for p in o.query] + [p.py for p in o.body_fields]
        dup = {n for n in names if names.count(n) > 1}
        if dup:
            die(f"{oid}：参数重名 {dup}")
        reserved = set(OPTION_PARAMS)
        if o.body_kind == "raw":
            reserved.add("body")
        if hints.get("upload"):
            reserved |= {"file", "filename", "content_type", "csv"}
        if oid == "create_page":
            reserved.add("markdown")
        if set(names) & reserved:
            die(f"{oid}：参数和 SDK 自己加的参数重名 {set(names) & reserved}")
        ops.append(o)
    return ops


@dataclass
class Node:
    path: List[str]
    children: Dict[str, "Node"] = field(default_factory=dict)
    ops: List[Op] = field(default_factory=list)

    @property
    def cls(self) -> str:
        # 加 Resource：不和模型（Agent……）重名
        return "".join(pascal(p) for p in self.path) + "Resource"


def build_tree(ops: List[Op]) -> Node:
    root = Node([])
    for o in ops:
        n = root
        for seg in o.name[:-1]:
            if seg not in n.children:
                n.children[seg] = Node(n.path + [seg])
            n = n.children[seg]
        n.ops.append(o)
    return root


def walk(n: Node) -> List[Node]:
    out: List[Node] = []
    for k in sorted(n.children):
        out += walk(n.children[k])
    out.append(n)
    return out


def item_type(sp: Spec, model: str, key: str) -> str:
    s = sp.schemas[model]["properties"][key]
    t, _ = py_type(sp, s)
    m = re.fullmatch(r"List\[(.+)\]", t)
    if not m:
        die(f"{model}.{key} 不是数组")
    return m.group(1)  # type: ignore[union-attr]


def path_expr(o: Op) -> str:
    p = o.path
    for pp in o.path_params:
        p = p.replace("{" + pp.name + "}", "{_p(" + pp.py + ")}")
    return ('f"' if o.path_params else '"') + p + '"'


def gen_method(sp: Spec, o: Op, is_async: bool) -> List[str]:
    meth = ident(o.name[-1])
    I = "        "
    sig: List[str] = ["self"]
    for pp in o.path_params:
        sig.append(f"{pp.py}: {pp.type}")
    upload = o.hints.get("upload")
    if upload == "file":
        sig += ["file: FileTypes", "filename: Optional[str] = None", "content_type: Optional[str] = None"]
    elif upload == "csv":
        sig.append("csv: Union[str, bytes]")
    sig.append("*")
    kw_req: List[str] = []
    kw_opt: List[str] = []
    for q in o.query:
        if q.required:
            kw_req.append(f"{q.py}: {q.type}")
        else:
            kw_opt.append(f"{q.py}: Union[{q.type}, NotGiven] = NOT_GIVEN")
    if o.body_kind == "fields":
        for f in o.body_fields:
            if f.required:
                kw_req.append(f"{f.py}: {f.type}")
            elif f.nullable:
                kw_opt.append(f"{f.py}: Union[{f.type}, None, NotGiven] = NOT_GIVEN")
            else:
                kw_opt.append(f"{f.py}: Union[{f.type}, NotGiven] = NOT_GIVEN")
    elif o.body_kind == "raw":
        if o.body_required:
            kw_req.append(f"body: {o.body_type}")
        else:
            kw_opt.append(f"body: Union[{o.body_type}, NotGiven] = NOT_GIVEN")
    if o.id == "create_page":
        kw_opt.append("markdown: Union[str, NotGiven] = NOT_GIVEN")
    sig += kw_req + kw_opt
    sig += [
        "extra_headers: Optional[Headers] = None",
        "extra_query: Optional[Query] = None",
    ]
    if o.body_kind == "fields":
        sig.append("extra_body: Optional[Body] = None")
    sig.append("timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN")

    # 返回类型
    hints = o.hints
    if o.resp == "sse":
        ret = "AsyncEventStream" if is_async else "EventStream"
    elif hints.get("paginate"):
        it = item_type(sp, o.resp_model or "", hints["paginate"])
        ret = f"AsyncPaginator[{it}]" if is_async else f"CursorPage[{it}]"
    elif hints.get("unwrap"):
        ret = sp.schemas[o.resp_model or ""]["properties"][hints["unwrap"]]
        ret = py_type(sp, ret)[0]
    elif o.resp == "model":
        ret = o.resp_model or "Any"
        if hints.get("null_on_404"):
            ret = f"Optional[{ret}]"
    elif o.resp == "bytes":
        ret = "bytes"
    elif o.resp == "json":
        ret = "Any"
    else:
        ret = "None"
    sync_like = o.resp == "sse" or bool(hints.get("paginate"))
    dfn = "async def" if is_async and not sync_like else "def"
    lines = [f"    {dfn} {meth}(", *[f"{I}{a}," for a in sig], f"    ) -> {ret}:"]
    doc = (o.summary or "").strip()
    if re.fullmatch(r"[A-Z]+ /\S*", doc):
        doc = ""  # 只有“方法 路径”的说明：下面本来就写
    head = f"`{o.method} /api/v1{o.path}`"
    lines += doc_lines((doc + "\n\n" + head) if doc else head, I)

    # 调用
    args: List[str] = [f'"{o.method}"', path_expr(o)]
    if o.query:
        args.append("query={" + ", ".join(f'"{q.name}": {q.py}' for q in o.query) + "}")
    if o.body_kind == "fields":
        args.append("body={" + ", ".join(f'"{f.name}": {f.py}' for f in o.body_fields) + "}")
        args.append("extra_body=extra_body")
    elif o.body_kind == "raw":
        args.append("body=body")
    if upload == "file":
        args.append("files=_file_part(file, filename, content_type)")
    elif upload == "csv":
        args.append("content=_csv_body(csv)")
        args.append('headers={"content-type": "text/csv; charset=utf-8"}')
    opts = ["extra_headers=extra_headers", "extra_query=extra_query", "timeout=timeout"]
    aw = "await " if is_async else ""
    if o.resp == "sse":
        lines.append(f"{I}return self._client._stream(")
        for a in [args[1], *[a for a in args[2:] if a.startswith("query=")], f'kind="{hints["sse"]}"', *opts]:
            lines.append(f"{I}    {a},")
        lines.append(f"{I})")
        return lines
    if hints.get("paginate"):
        it = item_type(sp, o.resp_model or "", hints["paginate"])
        lines.append(f"{I}return self._client._call_page(")
        for a in args + [f"model={o.resp_model}", f'items="{hints["paginate"]}"', f"item={it}"] + opts:
            lines.append(f"{I}    {a},")
        lines.append(f"{I})")
        return lines
    if hints.get("unwrap"):
        cast = ret
        args.append(f'unwrap="{hints["unwrap"]}"')
    elif o.resp == "model":
        cast = o.resp_model or "Any"
    elif o.resp == "bytes":
        cast = "bytes"
    elif o.resp == "json":
        cast = "object"
    else:
        cast = "type(None)"
    fn = "_call_or_none" if hints.get("null_on_404") else "_call"
    if o.id == "create_page":
        lines.append(f"{I}page = {aw}self._client.{fn}(")
    else:
        lines.append(f"{I}return {aw}self._client.{fn}(")
    for a in args + [f"cast={cast}"] + opts:
        lines.append(f"{I}    {a},")
    lines.append(f"{I})")
    if o.id == "create_page":
        lines += [
            f"{I}if not isinstance(markdown, NotGiven):",
            f"{I}    # 两步：先建页面，再写正文",
            f"{I}    {aw}self._client._call(",
            f'{I}        "PUT",',
            f'{I}        f"/pages/{{_p(page.id)}}/content",',
            f'{I}        body={{"markdown": markdown}},',
            f"{I}        cast=type(None),",
            *[f"{I}        {a}," for a in opts],
            f"{I}    )",
            f"{I}return page",
        ]
    return lines


def gen_resources(sp: Spec, ops: List[Op]) -> str:
    root = build_tree(ops)
    out: List[str] = []
    for is_async in (False, True):
        pre = "Async" if is_async else ""
        client_t = "AsyncAPIClient" if is_async else "SyncAPIClient"
        for n in walk(root):
            if not n.path:
                continue
            out += ["", "", f"class {pre}{n.cls}:"]
            out += doc_lines(f"`client.{'.'.join(n.path)}`", "    ")
            out += ["", f"    def __init__(self, client: {client_t}) -> None:", "        self._client = client"]
            for k in sorted(n.children):
                c = n.children[k]
                out += ["", "    @cached_property", f"    def {ident(k)}(self) -> {pre}{c.cls}:", f"        return {pre}{c.cls}(self._client)"]
            for o in n.ops:
                out.append("")
                out += gen_method(sp, o, is_async)
        # 客户端上的：第一层资源和只有一段名字的方法
        out += ["", "", f"class {'AsyncResources' if is_async else 'SyncResources'}({client_t}):"]
        out += doc_lines("客户端上的资源和方法（生成的）；Client / AsyncClient 继承它。", "    ")
        for k in sorted(root.children):
            c = root.children[k]
            out += ["", "    @cached_property", f"    def {ident(k)}(self) -> {pre}{c.cls}:", f"        return {pre}{c.cls}(self)"]
        for o in root.ops:
            out.append("")
            out += [ln.replace("self._client.", "self.") for ln in gen_method(sp, o, is_async)]
    out.append("")
    body = "\n".join(out)
    # 只 import 用到的（类型检查的 strict 模式不许有没用的 import）
    words = set(re.findall(r"\b\w+\b", re.sub(r'""".*?"""', "", body, flags=re.S)))
    typing_names = [n for n in ("Any", "Dict", "List", "Literal", "Mapping", "Optional", "Sequence", "Union") if n in words]
    model_names = sorted(n for n in sp.schemas if n in words and n not in typing_names)
    # 资源类调客户端的 _call 之类：同一个包里的“内部”方法
    head = [HEADER.rstrip("\n"), RESOURCES_PRAGMA.rstrip("\n"), '"""资源类：client.pages.versions.list(...) 这样的方法（名字见 sdks/operations.json）。"""', "from __future__ import annotations", ""]
    head += [
        "from functools import cached_property",
        "from typing import " + ", ".join(typing_names),
        "",
        "from .._base_client import AsyncAPIClient, Body, Headers, Query, SyncAPIClient, TimeoutTypes",
        "from .._files import FileTypes, csv_body as _csv_body, file_part as _file_part",
        "from .._pagination import AsyncPaginator, CursorPage",
        "from .._sse import AsyncEventStream, EventStream",
        "from .._types import NOT_GIVEN, NotGiven",
        "from .._utils import quote_path as _p",
        "from .models import (",
        *[f"    {n}," for n in model_names],
        ")",
        "",
        '__all__ = ["SyncResources", "AsyncResources"]',
    ]
    return "\n".join(head) + "\n" + body


INIT = HEADER + '"""生成的代码：模型（models）和资源类（resources）。"""\n'


def generate() -> Dict[Path, str]:
    spec = json.loads(SPEC_PATH.read_text(encoding="utf-8"))
    table = json.loads(OPS_PATH.read_text(encoding="utf-8"))
    sp = Spec(spec)
    ops = build_ops(sp, table)
    return {
        OUT_DIR / "__init__.py": INIT,
        OUT_DIR / "models.py": gen_models(sp),
        OUT_DIR / "resources.py": gen_resources(sp, ops),
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--check", action="store_true", help="只比较，不一致就退出 1")
    a = ap.parse_args()
    files = generate()
    if a.check:
        bad = False
        for p, text in files.items():
            old = p.read_text(encoding="utf-8") if p.exists() else ""
            if old != text:
                bad = True
                rel = p.relative_to(ROOT)
                print(f"✗ {rel} 不是最新的", file=sys.stderr)
                diff = list(difflib.unified_diff(old.splitlines(), text.splitlines(), str(rel), str(rel) + "（新生成）", lineterm="", n=1))
                for ln in diff[:40]:
                    print("  " + ln, file=sys.stderr)
        if bad:
            print("  重新生成：python sdks/python/codegen/generate.py", file=sys.stderr)
            return 1
        print(f"✓ Python SDK 的生成代码是最新的（{len(files)} 个文件）")
        return 0
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    for p, text in files.items():
        p.write_text(text, encoding="utf-8")
    print(f"✓ 生成了 {', '.join(str(p.relative_to(ROOT)) for p in files)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
