#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["httpx>=0.27", "psutil>=5.9", "playwright>=1.45"]
# ///
"""旧版（git 标签 python-final 里的 legacy/，Python + FastAPI + SQLite）的性能基线：给 Rust 重写定一组要超过的数。

在仓库根目录跑：

  uv run bench/bench.py --out /tmp/oad-baseline            # 全部
  uv run bench/bench.py --out ... --only coldstart,pages   # 只跑某几项
  uv run bench/bench.py --out ... --no-frontend            # 不开浏览器

做的事（结果写进 <out>/results.json，最后打印一张表）：
  footprint  在 <out>/venv 里装一份不带开发依赖的旧版（uv sync --no-dev --no-editable），量大小、import 时间
  coldstart  启动进程 → /healthz 通的时间，空闲内存（空数据目录 / 已有数据目录各几次）
  pages      1000 段的页面：建、网页打开要的三个请求、MCP read_page / edit_page / rewrite_page、网页按段保存
  search     500 页 × 200 段（约 10 万段）：第一次搜（建索引）、50 个中英文查询（网页接口和 MCP 各 3 轮）
  db         11 个属性、1 万行：db_add 批量插入、网页打开视图、筛选、排序、公式列，MCP db_query
  frontend   Playwright + Chromium：打开 1000 段页面、1 万行数据库视图出内容的时间，在长页面里打字的延迟

数据都是脚本按固定随机种子生成的（中英混合），不联网。不会打印密码和 token。

接口的数字有两种：客户端看到的墙上时间（p50 / p95），和服务进程每次请求花的 CPU 时间（server_cpu_ms）。
机器上别的东西忙的时候墙上时间能差两三倍，CPU 时间稳一些；每项结果都带着前后的 load average。
整套跑一次约 8 分钟（1 万行的插入占一半）。2026-11-18 三轮的结果在 results-2026-11-18.json，结论见 docs/baseline.md。
"""
import argparse
import gzip
import json
import os
import platform
import random
import re
import shutil
import socket
import statistics
import subprocess
import sys
import threading
import time
from pathlib import Path

import httpx
import psutil

REPO = Path(__file__).resolve().parents[2]
LEGACY = REPO / "legacy"
PW = "bench-password-1"
NOPROXY = {k: "" for k in ("http_proxy", "https_proxy", "all_proxy", "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY")} | {"NO_PROXY": "*", "no_proxy": "*"}
RESULTS: dict = {}


def log(*a):
    print(*a, flush=True)


# ---------- 统计 ----------
def pct(xs, p):
    s = sorted(xs)
    if not s:
        return None
    k = (len(s) - 1) * p / 100
    lo, hi = int(k), min(int(k) + 1, len(s) - 1)
    return s[lo] + (s[hi] - s[lo]) * (k - lo)


def summ(xs) -> dict:
    xs = list(xs)
    return {"n": len(xs), "p50": round(pct(xs, 50), 2), "p95": round(pct(xs, 95), 2), "min": round(min(xs), 2),
            "max": round(max(xs), 2), "mean": round(statistics.fmean(xs), 2)}


def timed(fn, n: int, warmup: int = 1) -> list[float]:
    for _ in range(warmup):
        fn(-1)
    out = []
    for i in range(n):
        t0 = time.perf_counter()
        fn(i)
        out.append((time.perf_counter() - t0) * 1000)
    return out


def measure(srv, fn, n: int, warmup: int = 1) -> dict:
    """墙上时间（客户端看到的）+ 服务进程每次花的 CPU 时间（user+sys）。机器上别的东西忙的时候，CPU 时间比墙上时间稳。"""
    for _ in range(warmup):
        fn(-1)
    c0 = srv.cpu()
    xs = timed(fn, n, warmup=0)
    return summ(xs) | {"server_cpu_ms": round((srv.cpu() - c0) * 1000 / n, 2)}


def mb(n) -> float:
    return round(n / 2**20, 1)


def du(p: Path) -> int:
    return sum(f.stat().st_size for f in p.rglob("*") if f.is_file() and not f.is_symlink())


def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


# ---------- 生成数据（中英混合，按词频大致服从 Zipf） ----------
ZH = ("项目 进度 会议 纪要 需求 设计 接口 数据库 用户 测试 发布 版本 性能 优化 搜索 文档 编辑器 评论 通知 权限 部署 服务器 缓存 索引 前端 后端 客户 合同 "
      "预算 季度 目标 指标 风险 计划 负责人 截止 讨论 结论 问题 方案 架构 模块 组件 页面 表格 列表 标题 段落 代码 图片 视频 链接 卡片 同步 冲突 合并 "
      "历史 回收站 分享 登录 密码 日志 监控 告警 故障 复盘 迁移 备份 恢复 存储 网络 延迟 吞吐 内存 磁盘 线程 并发 事务 查询 过滤 排序 公式 关联 汇总 "
      "视图 看板 日历 画廊 模板 导入 导出 翻译 智能体 模型 提示词 上下文 工具 调用 结果 反馈 体验 原型 交互 动画 主题 字体 颜色 移动端 桌面端 浏览器 "
      "插件 市场 销售 运营 财务 招聘 培训 周报 月报 总结 北京 上海 深圳 杭州 成都 产品经理 工程师 设计师 实习生 供应商 采购 库存 物流 仓库 订单 发票 "
      "报销 审批 流程 自动化 脚本 机器人 爬虫 数据集 训练 推理 评测 基准 压测 容量 扩容 降级 熔断 限流 灰度 回滚 证书 域名 防火墙 漏洞 加密 签名 "
      "协作 实时 离线 草稿 快照 撤销 重做 快捷键 拖拽 粘贴 嵌入 公众号 小程序 支付 退款 会员 积分 优惠券 活动 增长 留存 转化 漏斗 埋点 报表 仪表盘").split()
ZH_GLUE = "的 了 和 在 我们 需要 已经 可以 这个 下周 今天 明天 完成 开始 继续 确认 检查 更新 修复 增加 删除 调整 讨论 关于 还有 因为 所以 但是 如果".split()
EN = ("project roadmap meeting notes requirement design api database user testing release version performance latency search document editor "
      "comment notification permission deploy server cache index frontend backend customer contract budget quarter goal metric risk plan owner "
      "deadline discussion decision issue proposal architecture module component page table list heading paragraph code image video link card "
      "sync conflict merge history trash share login password token logging monitoring alert incident postmortem migration backup restore storage "
      "network throughput memory disk thread concurrency transaction query filter sort formula relation rollup view board calendar gallery template "
      "import export translation agent model prompt context tool result feedback prototype interaction animation theme font color mobile desktop "
      "browser plugin market sales operations finance hiring onboarding weekly monthly summary vendor procurement inventory shipping warehouse order "
      "invoice expense approval workflow automation script crawler dataset training inference benchmark capacity rollback certificate firewall "
      "vulnerability encryption realtime offline draft snapshot undo shortcut embed payment refund membership coupon campaign growth retention "
      "conversion funnel dashboard kubernetes postgres sqlite rust python typescript react tiptap websocket crdt tantivy jieba").split()
EN_GLUE = "the a of to and in for with on is we need should will this next today tomorrow after before because so but if".split()
ZW = [1 / (i + 1) ** 0.9 for i in range(len(ZH))]
EW = [1 / (i + 1) ** 0.9 for i in range(len(EN))]


def zh_sentence(r: random.Random, n: int | None = None) -> str:
    n = n or r.randint(6, 16)
    out = []
    for _ in range(n):
        x = r.random()
        out.append(r.choice(ZH_GLUE) if x < 0.3 else r.choices(EN, EW)[0] if x < 0.37 else r.choices(ZH, ZW)[0])
    s = ""
    for w in out:   # 中文之间不加空格，英文词两边加
        s += (f" {w} " if w.isascii() else w)
    return " ".join(s.split()) + r.choice("。。。！？")


def en_sentence(r: random.Random, n: int | None = None) -> str:
    n = n or r.randint(8, 20)
    ws = [r.choice(EN_GLUE) if r.random() < 0.35 else r.choices(EN, EW)[0] for _ in range(n)]
    if r.random() < 0.15:
        ws.insert(r.randrange(len(ws)), r.choices(ZH, ZW)[0])
    s = " ".join(ws)
    return s[0].upper() + s[1:] + "."


def sentence(r, n=None):
    return zh_sentence(r, n) if r.random() < 0.6 else en_sentence(r, n)


def para(r):
    return " ".join(sentence(r) for _ in range(r.randint(1, 3)))


CODE = ["```python\ndef handler(req):\n    rows = db.query(\"select * from pages where id = ?\", req.id)\n    return {\"ok\": True, \"rows\": rows}\n```",
        "```rust\nfn main() {\n    let index = Index::open(\"data\").unwrap();\n    println!(\"{} docs\", index.len());\n}\n```",
        "```ts\nexport async function load(id: number) {\n  const r = await fetch(`/api/pages/${id}`)\n  return r.json()\n}\n```",
        "```sh\nuv run open-agent-docs serve --port 8790\n```"]


def make_doc(r: random.Random, target: int, start: str = "", end: str = "") -> tuple[str, int]:
    """大约 target 段的 Markdown（按服务器的切法数：列表的每项、表格的每行各算一段）。返回 (正文, 估计的段数)。"""
    groups, n = [], 0
    if start:
        groups.append(f"# {start} {zh_sentence(r, 5)[:-1]}")
        n += 1
    while n < target - 1:
        x = r.random()
        if x < 0.40:
            groups.append(para(r)); n += 1
        elif x < 0.48:
            groups.append("#" * r.choice((2, 2, 3)) + " " + sentence(r, r.randint(3, 6))[:-1]); n += 1
        elif x < 0.66:
            k = r.randint(3, 6)
            groups.append("\n".join(f"- {sentence(r)}" for _ in range(k))); n += k
        elif x < 0.74:
            k = r.randint(3, 5)
            groups.append("\n".join(f"{i + 1}. {sentence(r)}" for i in range(k))); n += k
        elif x < 0.79:
            k = r.randint(2, 4)
            groups.append("\n".join(f"- [{'x' if r.random() < 0.4 else ' '}] {sentence(r, 6)}" for _ in range(k))); n += k
        elif x < 0.89:
            k = r.randint(3, 8)
            head = "| 名称 | Owner | 状态 | 金额 |\n| --- | --- | --- | ---: |"
            rows = [f"| {r.choices(ZH, ZW)[0]}{r.choices(ZH, ZW)[0]} | {r.choice(EN)} | {r.choice(['进行中', 'done', '未开始'])} | {r.randint(1, 99999)} |"
                    for _ in range(k)]
            groups.append("\n".join([head] + rows)); n += k + 1
        elif x < 0.94:
            groups.append(r.choice(CODE)); n += 1
        else:
            groups.append("> " + sentence(r)); n += 1
    groups.append(f"{para(r)} {end}".strip())
    n += 1
    return "\n\n".join(groups), n


def make_queries(r: random.Random) -> list[str]:
    """50 个查询：中文 25（常用词、少见词、两三个词）、英文 25（同样）。"""
    zh = ZH[:8] + ZH[60:68] + [f"{ZH[i]}{ZH[i + 7]}" for i in range(0, 35, 7)] + ["数据库 性能", "搜索 索引 延迟", "会议 纪要 结论", "智能体 提示词"]
    en = EN[:8] + EN[60:68] + [f"{EN[i]} {EN[i + 9]}" for i in range(0, 45, 9)] + ["database performance", "search index latency",
                                                                                 "agent prompt context", "API 接口"]
    return zh[:25] + en[:25]


# ---------- 服务 ----------
class Server:
    def __init__(self, exe: Path, data: Path, log_path: Path):
        self.exe, self.data, self.log_path = exe, data, log_path
        self.port = free_port()
        self.base = f"http://127.0.0.1:{self.port}"
        self.proc = None
        self.peak = 0
        self._stop = threading.Event()

    def start(self) -> float:
        env = {**os.environ, **NOPROXY, "OAD_PASSWORD": PW, "OAD_LANG": "zh", "PYTHONUNBUFFERED": "1"}
        self.data.mkdir(parents=True, exist_ok=True)
        hc = httpx.Client(trust_env=False, timeout=1)
        t0 = time.perf_counter()
        self.proc = subprocess.Popen([str(self.exe), "serve", "--port", str(self.port), "--data", str(self.data)], env=env,
                                     stdout=open(self.log_path, "a"), stderr=subprocess.STDOUT, cwd=self.data)
        while True:
            try:
                if hc.get(self.base + "/healthz").status_code == 200:
                    break
            except httpx.HTTPError:
                pass
            if self.proc.poll() is not None:
                raise RuntimeError(f"server exited, see {self.log_path}")
            if time.perf_counter() - t0 > 60:
                raise RuntimeError("server did not start in 60s")
            time.sleep(0.002)
        dt = (time.perf_counter() - t0) * 1000
        hc.close()
        self.ps = psutil.Process(self.proc.pid)
        self._stop.clear()
        threading.Thread(target=self._sample, daemon=True).start()
        return dt

    def _sample(self):
        while not self._stop.is_set():
            try:
                self.peak = max(self.peak, self.ps.memory_info().rss)
            except psutil.Error:
                return
            time.sleep(0.05)

    def cpu(self) -> float:
        t = self.ps.cpu_times()
        return t.user + t.system

    def rss(self) -> int:
        return self.ps.memory_info().rss

    def stop(self):
        self._stop.set()
        if self.proc and self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(10)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait(5)


class Client:
    """网页那边（登录 cookie）和 Agent 那边（MCP，Bearer token）。token 只放在内存里。"""

    def __init__(self, base: str):
        self.c = httpx.Client(base_url=base, trust_env=False, timeout=600)
        r = self.c.post("/api/login", json={"password": PW})
        r.raise_for_status()
        self._tok = None
        self._n = 0

    def agent(self, name: str, scope: str = "write") -> str:
        r = self.c.post("/api/agents", json={"name": name, "scope": scope})
        r.raise_for_status()
        return r.json()["token"]

    def use_agent(self, name: str = "bench"):
        self._tok = self.agent(name)

    def get(self, path, **kw):
        r = self.c.get(path, **kw)
        r.raise_for_status()
        return r

    def send(self, method, path, body):
        r = self.c.request(method, path, json=body)
        r.raise_for_status()
        return r.json()

    def mcp(self, tool: str, args: dict) -> str:
        self._n += 1
        r = self.c.post("/mcp", json={"jsonrpc": "2.0", "id": self._n, "method": "tools/call", "params": {"name": tool, "arguments": args}},
                        headers={"authorization": f"Bearer {self._tok}", "accept": "application/json, text/event-stream",
                                 "mcp-protocol-version": "2025-06-18"})
        r.raise_for_status()
        raw = r.text
        if not raw.lstrip().startswith("{"):
            raw = [x[5:].strip() for x in raw.splitlines() if x.startswith("data:")][-1]
        msg = json.loads(raw)
        if "error" in msg:
            raise RuntimeError(f"{tool}: {msg['error']}")
        res = msg["result"]
        text = "".join(c.get("text", "") for c in res.get("content", []))
        if res.get("isError"):
            raise RuntimeError(f"{tool}: {text[:300]}")
        return text


# ---------- 各项 ----------
def venv_setup(out: Path) -> Path:
    venv = out / "venv"
    if not (venv / "bin" / "open-agent-docs").exists():
        log("· 装一份不带开发依赖的旧版到", venv)
        env = {**os.environ, "UV_PROJECT_ENVIRONMENT": str(venv)}
        subprocess.run(["uv", "sync", "--project", str(LEGACY), "--frozen", "--no-dev", "--no-editable", "-q"], env=env, check=True)
    return venv


def bench_footprint(out: Path, venv: Path):
    log("== footprint")
    sp = next((venv / "lib").glob("python*/site-packages"))
    static = sp / "open_agent_docs" / "static"
    if not static.is_dir():
        static = LEGACY / "open_agent_docs" / "static"
    assets = {}
    for f in sorted(static.rglob("*")):
        if f.is_file() and f.suffix in (".js", ".css", ".html"):
            raw = f.read_bytes()
            assets[f.name] = {"bytes": len(raw), "gzip": len(gzip.compress(raw, 6))}
    pkgs = sorted(((d.name, du(d)) for d in sp.iterdir() if d.is_dir()), key=lambda x: -x[1])[:8]
    py = venv / "bin" / "python"
    env = {**os.environ, "OAD_DATA_DIR": str(out / "data-import"), "OAD_PASSWORD": PW}
    imp = []
    for _ in range(6):
        r = subprocess.run([str(py), "-c", "import time; t=time.perf_counter(); import open_agent_docs.server; print((time.perf_counter()-t)*1000)"],
                           env=env, capture_output=True, text=True, check=True)
        imp.append(float(r.stdout.strip().splitlines()[-1]))
    pyver = subprocess.run([str(py), "-c", "import sys; print(sys.version.split()[0])"], capture_output=True, text=True).stdout.strip()
    interp = Path(os.path.realpath(py)).parents[1]
    RESULTS["footprint"] = {
        "python": pyver, "venv_bytes": du(venv), "venv_site_packages_bytes": du(sp),
        "python_runtime_bytes": du(interp), "python_runtime_path": str(interp),
        "static_bytes": du(static), "assets": assets, "biggest_packages": [{"name": n, "bytes": b} for n, b in pkgs],
        "import_server_ms": summ(imp[1:]),
    }
    log(f"  venv {mb(du(venv))} MB, static {mb(du(static))} MB, import {summ(imp[1:])['p50']} ms")


def bench_coldstart(out: Path, venv: Path, runs: int):
    log("== coldstart")
    exe = venv / "bin" / "open-agent-docs"
    res = {}
    for mode in ("fresh", "existing"):
        ts, rss0, rss_idle, cpu0 = [], [], [], []
        keep = out / "data-cold-existing"
        for i in range(runs + 1):   # 第一次不算（.pyc、磁盘缓存）
            data = out / f"data-cold-{mode}-{i}" if mode == "fresh" else keep
            if mode == "fresh":
                shutil.rmtree(data, ignore_errors=True)
            s = Server(exe, data, out / "server-cold.log")
            dt = s.start()
            r0, k0 = s.rss(), s.cpu()
            time.sleep(3)
            r1 = s.rss()
            s.stop()
            if i:
                ts.append(dt); rss0.append(r0); rss_idle.append(r1); cpu0.append(k0 * 1000)
            if mode == "fresh":
                shutil.rmtree(data, ignore_errors=True)
        res[mode] = {"healthz_ms": summ(ts), "rss_at_ready_mb": summ([mb(x) for x in rss0]), "rss_idle_3s_mb": summ([mb(x) for x in rss_idle]),
                     "cpu_to_ready_ms": summ(cpu0)}
        log(f"  {mode}: ready p50 {res[mode]['healthz_ms']['p50']} ms, idle RSS p50 {res[mode]['rss_idle_3s_mb']['p50']} MB")
    # 用 uv run 启动（README 里写的方式），多出来的是 uv 自己检查环境的时间
    ts = []
    for i in range(4):
        data = out / f"data-cold-uv-{i}"
        port = free_port()
        hc = httpx.Client(trust_env=False, timeout=1)
        t0 = time.perf_counter()
        p = subprocess.Popen(["uv", "run", "--project", str(LEGACY), "--frozen", "--no-dev", "open-agent-docs", "serve", "--port", str(port),
                              "--data", str(data)], env={**os.environ, **NOPROXY, "OAD_PASSWORD": PW, "UV_PROJECT_ENVIRONMENT": str(venv)},
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        while True:
            try:
                if hc.get(f"http://127.0.0.1:{port}/healthz").status_code == 200:
                    break
            except httpx.HTTPError:
                time.sleep(0.002)
        if i:
            ts.append((time.perf_counter() - t0) * 1000)
        p.terminate(); p.wait(10)
        shutil.rmtree(data, ignore_errors=True)
    res["via_uv_run_ms"] = summ(ts)
    RESULTS["coldstart"] = res


def fe_browser():
    from playwright.sync_api import sync_playwright
    pw = sync_playwright().start()
    return pw, pw.chromium.launch(args=["--no-proxy-server"])


FE_INIT = """(() => {
  const B = window.__bench = {};
  const mark = k => { if (B[k] === undefined) { B[k] = performance.now(); requestAnimationFrame(() => setTimeout(() => { B[k + '_frame'] = performance.now() }, 0)) } };
  try { new PerformanceObserver(l => { for (const e of l.getEntries()) { B.longtask = (B.longtask || 0) + e.duration; B.longtask_n = (B.longtask_n || 0) + 1 } })
          .observe({ type: 'longtask', buffered: true }) } catch (e) {}
  const check = () => {
    const md = document.querySelector('.page-md');   // 编辑器那一块还没加载完时，先用只读的 Markdown 显示
    if (md && md.firstElementChild && md.firstElementChild.textContent.includes('BENCHSTART')) mark('readonly');
    const ed = document.querySelector('.doc-body.ProseMirror');
    if (ed) {
      if (ed.firstElementChild && ed.firstElementChild.textContent.includes('BENCHSTART')) mark('first');
      if (ed.lastElementChild && ed.lastElementChild.textContent.includes('BENCHEND')) mark('full');
    }
    const n = document.querySelectorAll('table.db-table tbody tr').length;
    if (n >= 1) mark('first');
    if (n >= (window.__benchRows || 1e9)) mark('full');
  };
  new MutationObserver(check).observe(document, { subtree: true, childList: true, characterData: true });
})();"""


def fe_open(browser, base: str, path: str, runs: int, rows: int | None = None) -> dict:
    """打开 path 到出第一段 / 全部内容的时间（从导航开始算，页面里的 performance.now()）。冷：每次新的浏览器上下文（没缓存）；热：同一个上下文再开。"""
    out = {}
    init = FE_INIT + (f"\nwindow.__benchRows = {rows};" if rows else "")
    for mode in ("cold", "warm"):
        rec = {k: [] for k in ("fcp", "readonly", "readonly_frame", "first", "first_frame", "full", "full_frame", "longtask")}
        ctx = None
        for i in range(runs + (1 if mode == "warm" else 0)):
            if mode == "cold" or ctx is None:
                if ctx:
                    ctx.close()
                ctx = browser.new_context(viewport={"width": 1320, "height": 900}, locale="zh-CN")
                ctx.request.post(base + "/api/login", data={"password": PW})
                ctx.add_init_script(init)
            page = ctx.new_page()
            page.goto(base + path, wait_until="commit")
            page.wait_for_function("window.__bench && window.__bench.full_frame !== undefined", timeout=120000, polling=50)
            page.wait_for_timeout(500)
            b = page.evaluate("({...window.__bench, fcp: (performance.getEntriesByName('first-contentful-paint')[0] || {}).startTime})")
            page.close()
            if mode == "warm" and i == 0:
                continue   # 先开一次把静态文件放进缓存
            for k in rec:
                if b.get(k) is not None:
                    rec[k].append(b[k])
        ctx.close()
        out[mode] = {k: summ(v) for k, v in rec.items() if v}
    return out


FE_TYPE = """(() => {
  const L = window.__keys = [];
  document.addEventListener('keydown', e => { const t0 = e.timeStamp; requestAnimationFrame(() => setTimeout(() => L.push(performance.now() - t0), 0)) }, true);
})();"""


def fe_typing(browser, base: str, pid: int, n: int = 80) -> dict:
    """在页面最后一段末尾打字：每次按键从 keydown 到下一帧画完的时间。"""
    ctx = browser.new_context(viewport={"width": 1320, "height": 900}, locale="zh-CN")
    ctx.request.post(base + "/api/login", data={"password": PW})
    page = ctx.new_page()
    page.goto(f"{base}/pages/{pid}")
    ed = page.locator(".doc-body.ProseMirror")
    ed.wait_for(timeout=60000)
    page.wait_for_timeout(1500)
    last = ed.locator(":scope > *").last
    last.scroll_into_view_if_needed()
    last.click()
    page.keyboard.press("End")
    page.evaluate(FE_TYPE)
    for ch in ("typing latency test 打字延迟测试 " * 5)[:n]:
        page.keyboard.type(ch)
        page.wait_for_timeout(40)
    page.wait_for_timeout(1500)
    lat = page.evaluate("window.__keys")
    ctx.close()
    return summ(lat[5:])   # 前几下有聚焦、滚动的开销


def bench_pages(out: Path, venv: Path, n: int, frontend: bool):
    log("== pages")
    data = out / "data-pages"
    shutil.rmtree(data, ignore_errors=True)
    s = Server(venv / "bin" / "open-agent-docs", data, out / "server-pages.log")
    s.start()
    c = Client(s.base)
    c.use_agent("bench")
    r = random.Random(1000)
    content, est = make_doc(r, 1000, "BENCHSTART", "BENCHEND")
    res: dict = {"content_chars": len(content), "content_bytes": len(content.encode())}
    pid_box = []

    def create(i):
        pid_box.append(c.send("POST", "/api/pages", {"title": f"千段长页 {i}", "content": content})["id"])

    res["create_web_ms"] = measure(s, create, 5, warmup=0)
    pid = pid_box[0]
    blk = c.get(f"/api/pages/{pid}/blocks").json()
    order, version = blk["order"], blk["version"]
    res["blocks"] = len(order)
    log(f"  page #{pid}: {len(order)} blocks, {len(content)} chars")
    sizes = {}
    for name, path in (("page", f"/api/pages/{pid}"), ("blocks", f"/api/pages/{pid}/blocks"), ("comments", f"/api/pages/{pid}/comments")):
        sizes[name] = len(c.get(path).content)
        res[f"get_{name}_ms"] = measure(s, lambda i, p=path: c.get(p), n)
    res["response_bytes"] = sizes

    def ui_open(i):
        c.get(f"/api/pages/{pid}"); c.get(f"/api/pages/{pid}/blocks"); c.get(f"/api/pages/{pid}/comments")

    res["ui_open_3_requests_ms"] = measure(s, ui_open, n)
    res["mcp_read_page_first_ms"] = measure(s, lambda i: c.mcp("read_page", {"page_id": pid}), n)
    calls = []

    def read_all(i):
        start, k = None, 0
        while True:
            t = c.mcp("read_page", {"page_id": pid, **({"start": start} if start else {})})
            k += 1
            m = re.search(r'start="([A-Za-z0-9]+)"\)', t)
            if not m:
                break
            start = m.group(1)
        calls.append(k)

    res["mcp_read_page_full_ms"] = measure(s, read_all, max(5, n // 3))
    res["mcp_read_page_full_calls"] = calls[-1]
    mid = order[len(order) // 2]
    v = [version]

    def mcp_edit(i):
        t = c.mcp("edit_page", {"page_id": pid, "version": v[0], "ops": [{"op": "replace", "block": mid, "markdown": f"Agent 改的第 {i} 版 paragraph {time.time()}"}]})
        if "已改" not in t:
            raise RuntimeError("edit_page: " + t[:200])
        v[0] = int(re.search(r"现在是版本 (\d+)", t).group(1))

    res["mcp_edit_page_1_block_ms"] = measure(s, mcp_edit, n)
    v[0] = c.get(f"/api/pages/{pid}/blocks").json()["version"]
    other = order[len(order) // 3]

    def web_edit(i):
        v[0] = c.send("POST", f"/api/pages/{pid}/blocks", {"version": v[0], "ops": [{"op": "replace", "block": other, "markdown": f"网页上打字 {i} {time.time()}"}]})["version"]

    res["web_save_1_block_ms"] = measure(s, web_edit, n)
    lines = content.split("\n\n")

    def mcp_rewrite(i):
        cur = c.get(f"/api/pages/{pid}/blocks").json()["version"]
        new = list(lines)
        for k in (10, len(new) // 2, len(new) - 5):
            new[k] = f"重写的段落 {i}-{k} rewritten {time.time()}"
        t0 = time.perf_counter()
        t = c.mcp("rewrite_page", {"page_id": pid, "version": cur, "content": "\n\n".join(new)})
        if "已保存" not in t:
            raise RuntimeError("rewrite_page: " + t[:200])
        rw.append((time.perf_counter() - t0) * 1000)

    rw: list = []
    mcp_rewrite(-1)
    c0 = s.cpu()
    timed(mcp_rewrite, max(5, n // 3), warmup=0)
    res["mcp_rewrite_page_ms"] = summ(rw[1:]) | {"server_cpu_ms_incl_get_blocks": round((s.cpu() - c0) * 1000 / (len(rw) - 1), 2)}
    res["rss_after_mb"] = mb(s.rss())
    res["rss_peak_mb"] = mb(s.peak)
    res["db_file_mb"] = mb(sum(f.stat().st_size for f in data.glob("docs.db*")))
    if frontend:
        try:
            pw, browser = fe_browser()
            fresh = c.send("POST", "/api/pages", {"title": "千段长页（浏览器）", "content": content})["id"]
            small = c.send("POST", "/api/pages", {"title": "短页", "content": make_doc(random.Random(7), 10, "BENCHSTART", "BENCHEND")[0]})["id"]
            res["frontend_open_1000"] = fe_open(browser, s.base, f"/pages/{fresh}", 5)
            res["frontend_open_10"] = fe_open(browser, s.base, f"/pages/{small}", 5)
            res["frontend_typing_1000"] = fe_typing(browser, s.base, fresh)
            res["frontend_typing_10"] = fe_typing(browser, s.base, small)
            browser.close(); pw.stop()
        except Exception as e:   # 浏览器这块是尽力而为
            res["frontend_error"] = f"{type(e).__name__}: {e}"[:500]
            log("  frontend failed:", res["frontend_error"])
    s.stop()
    RESULTS["pages"] = res
    log(f"  ui open p50 {res['ui_open_3_requests_ms']['p50']} ms, read_page {res['mcp_read_page_first_ms']['p50']} ms, "
        f"edit_page {res['mcp_edit_page_1_block_ms']['p50']} ms, rewrite {res['mcp_rewrite_page_ms']['p50']} ms")


def bench_search(out: Path, venv: Path, pages: int, per_page: int, rounds: int):
    log("== search")
    data = out / "data-search"
    shutil.rmtree(data, ignore_errors=True)
    s = Server(venv / "bin" / "open-agent-docs", data, out / "server-search.log")
    s.start()
    c = Client(s.base)
    c.use_agent("bench")
    r = random.Random(2024)
    t0 = time.perf_counter()
    total_blocks, ids, chars = 0, [], 0
    for i in range(pages):
        content, nb = make_doc(r, per_page)
        title = f"{r.choices(ZH, ZW)[0]}{r.choice(['周报', '纪要', '方案', '复盘', '需求'])} {r.choice(EN)} #{i}"
        ids.append(c.send("POST", "/api/pages", {"title": title, "content": content})["id"])
        total_blocks += nb
        chars += len(content)
    load_s = time.perf_counter() - t0
    real_blocks = sum(len(c.get(f"/api/pages/{p}/blocks").json()["order"]) for p in ids[:20]) / 20 * pages
    res: dict = {"pages": pages, "blocks_est": round(real_blocks), "chars": chars, "load_s": round(load_s, 1),
                 "rss_before_index_mb": mb(s.rss())}
    log(f"  loaded {pages} pages (~{round(real_blocks)} blocks, {chars} chars) in {load_s:.1f}s")
    qs = make_queries(random.Random(5))
    t0, c0 = time.perf_counter(), s.cpu()
    c.get("/api/search", params={"q": qs[0]}).json()
    res["first_query_index_ms"] = round((time.perf_counter() - t0) * 1000, 1)
    res["first_query_index_server_cpu_ms"] = round((s.cpu() - c0) * 1000, 1)
    res["rss_after_index_mb"] = mb(s.rss())
    log(f"  first query (builds index) {res['first_query_index_ms']} ms")
    ui = {"zh": [], "en": []}
    hits = []
    c0 = s.cpu()
    for _ in range(rounds):
        for k, q in enumerate(qs):
            t0 = time.perf_counter()
            h = c.get("/api/search", params={"q": q}).json()
            ui["zh" if k < 25 else "en"].append((time.perf_counter() - t0) * 1000)
            hits.append(len(h))
    res["ui_search_ms"] = summ(ui["zh"] + ui["en"]) | {"server_cpu_ms": round((s.cpu() - c0) * 1000 / len(hits), 2)}
    res["ui_search_zh_ms"] = summ(ui["zh"])
    res["ui_search_en_ms"] = summ(ui["en"])
    res["ui_hits_avg"] = round(statistics.fmean(hits), 1)
    per_q = {}
    for q in qs:   # 每个查询单独的中位数，找最慢的
        per_q[q] = pct(timed(lambda i, q=q: c.get("/api/search", params={"q": q}), 3, warmup=0), 50)
    res["slowest_queries"] = [{"q": q, "ms": round(t, 1)} for q, t in sorted(per_q.items(), key=lambda x: -x[1])[:5]]
    res["fastest_queries"] = [{"q": q, "ms": round(t, 1)} for q, t in sorted(per_q.items(), key=lambda x: x[1])[:3]]
    mt = []
    c0 = s.cpu()
    for _ in range(rounds):
        for q in qs:
            t0 = time.perf_counter()
            c.mcp("search_pages", {"query": q})
            mt.append((time.perf_counter() - t0) * 1000)
    res["mcp_search_ms"] = summ(mt) | {"server_cpu_ms": round((s.cpu() - c0) * 1000 / len(mt), 2)}
    res["no_hit_query_ms"] = measure(s, lambda i: c.get("/api/search", params={"q": "zzqxw"}), 20)
    # 改一页之后的下一次搜索：sync() 要把这一页重新切块、分词
    after = []
    for i in range(8):
        pid = ids[(i * 7) % len(ids)]
        b = c.get(f"/api/pages/{pid}/blocks").json()
        c.send("POST", f"/api/pages/{pid}/blocks", {"version": b["version"], "ops": [{"op": "replace", "block": b["order"][3], "markdown": f"改了一段 {i}"}]})
        t0 = time.perf_counter()
        c.get("/api/search", params={"q": qs[i]})
        after.append((time.perf_counter() - t0) * 1000)
    res["search_after_one_edit_ms"] = summ(after)
    res["rss_after_mb"] = mb(s.rss())
    res["rss_peak_mb"] = mb(s.peak)
    res["db_file_mb"] = mb(sum(f.stat().st_size for f in data.glob("docs.db*")))
    s.stop()
    # 有数据时的冷启动（10 万段的库）
    s2 = Server(venv / "bin" / "open-agent-docs", data, out / "server-search.log")
    res["coldstart_with_corpus_ms"] = round(s2.start(), 1)
    res["rss_idle_with_corpus_mb"] = mb(s2.rss())
    s2.stop()
    RESULTS["search"] = res
    log(f"  ui search p50 {res['ui_search_ms']['p50']} p95 {res['ui_search_ms']['p95']} ms; mcp p50 {res['mcp_search_ms']['p50']} p95 {res['mcp_search_ms']['p95']}")


def bench_db(out: Path, venv: Path, nrows: int, n: int, frontend: bool):
    log("== db")
    data = out / "data-db"
    shutil.rmtree(data, ignore_errors=True)
    s = Server(venv / "bin" / "open-agent-docs", data, out / "server-db.log")
    s.start()
    c = Client(s.base)
    c.use_agent("bench")
    people = ["Nova", "Atlas", "小明", "Iris"]
    for p in people:
        c.agent(p)
    r = random.Random(77)
    t = c.mcp("db_create", {"title": "项目", "properties": [{"name": "预算", "type": "number"}]})
    proj = int(re.search(r"#(\d+)", t).group(1))
    c.mcp("db_add", {"db_id": proj, "rows": [{"title": f"项目 {r.choice(ZH)} {i}", "props": {"预算": r.randint(1, 100) * 1000}} for i in range(50)]})
    proj_rows = [x["id"] for x in c.get(f"/api/dbs/{proj}", params={"limit": 100}).json()["rows"]]
    cats = ["研发", "设计", "市场", "销售", "运营", "财务", "法务", "客服"]
    props = [{"name": "描述", "type": "text"}, {"name": "单价", "type": "number", "format": "cny"}, {"name": "数量", "type": "number"},
             {"name": "类别", "type": "select", "options": cats}, {"name": "状态", "type": "status"}, {"name": "截止", "type": "date"},
             {"name": "负责人", "type": "person"}, {"name": "完成", "type": "checkbox"}, {"name": "网址", "type": "url"},
             {"name": "项目", "type": "relation", "db": proj}, {"name": "总价", "type": "formula", "formula": 'prop("单价") * prop("数量")', "format": "cny"}]
    t = c.mcp("db_create", {"title": "任务（1 万行）", "properties": props})
    dbid = int(re.search(r"#(\d+)", t).group(1))
    res: dict = {"props": len(props) + 1, "prop_types": [p["type"] for p in props]}
    statuses = ["未开始", "进行中", "已完成"]
    batch, calls = 50, []
    t_all, c0 = time.perf_counter(), s.cpu()
    for b in range(nrows // batch):
        rows = []
        for k in range(batch):
            rows.append({"title": f"{zh_sentence(r, 4)[:-1]} {b * batch + k}", "props": {
                "描述": sentence(r, 8), "单价": round(r.uniform(1, 1000), 2), "数量": r.randint(1, 50), "类别": r.choice(cats),
                "状态": r.choice(statuses), "截止": f"2026-{r.randint(1, 12):02d}-{r.randint(1, 28):02d}", "负责人": r.choice(people),
                "完成": r.random() < 0.3, "网址": f"https://example.com/{r.choice(EN)}/{b}-{k}", "项目": [r.choice(proj_rows)]}})
        t0 = time.perf_counter()
        c.mcp("db_add", {"db_id": dbid, "rows": rows})
        calls.append((time.perf_counter() - t0) * 1000)
        if b % 40 == 0:
            log(f"  db_add batch {b}: {calls[-1]:.0f} ms")
    res["insert_total_s"] = round(time.perf_counter() - t_all, 1)
    res["insert_rows_per_s"] = round(nrows / res["insert_total_s"], 1)
    res["insert_server_cpu_s"] = round(s.cpu() - c0, 1)
    res["db_add_50_rows_ms"] = summ(calls)
    res["db_add_first10_p50_ms"] = round(pct(calls[:10], 50), 1)
    res["db_add_last10_p50_ms"] = round(pct(calls[-10:], 50), 1)
    log(f"  inserted {nrows} rows in {res['insert_total_s']}s")
    d = c.get(f"/api/dbs/{dbid}", params={"limit": 1}).json()
    key = {p["name"]: p for p in d["schema"]}
    doing = next(o["id"] for o in key["状态"]["options"] if o["name"] == "进行中")
    base_views = d["views"]
    extra = [{"id": "vf", "name": "筛选", "type": "table", "filter": [{"prop": key["状态"]["key"], "op": "eq", "value": doing},
                                                                       {"prop": key["单价"]["key"], "op": "gt", "value": 500}]},
             {"id": "vs", "name": "排序", "type": "table", "sort": [{"prop": key["单价"]["key"], "desc": True}]},
             {"id": "vfm", "name": "公式", "type": "table", "filter": [{"prop": key["总价"]["key"], "op": "gt", "value": 20000}],
              "sort": [{"prop": key["总价"]["key"], "desc": True}]}]
    c.send("PUT", f"/api/dbs/{dbid}/views", {"views": base_views + extra})
    vids = {v["name"]: v["id"] for v in c.get(f"/api/dbs/{dbid}", params={"limit": 1}).json()["views"]}
    table, board = base_views[0]["id"], base_views[1]["id"] if len(base_views) > 1 else None
    first = c.get(f"/api/dbs/{dbid}", params={"view": table, "limit": 200})
    res["view_response_bytes"] = len(first.content)
    for name, vid in (("view_table", table), ("view_board", board), ("view_filter", vids["筛选"]), ("view_sort", vids["排序"]),
                      ("view_formula_filter_sort", vids["公式"])):
        if vid:
            got = c.get(f"/api/dbs/{dbid}", params={"view": vid, "limit": 200}).json()
            res[name + "_count"] = got["count"]
            res[name + "_ms"] = measure(s, lambda i, v=vid: c.get(f"/api/dbs/{dbid}", params={"view": v, "limit": 200}), n)
    allv = c.get(f"/api/dbs/{dbid}", params={"limit": 1}).json()["views"]

    def change_filter(i):   # 网页上改筛选：PUT 整个视图列表，回来的是按新视图的前 200 行
        vs = json.loads(json.dumps(allv))
        f = next(v for v in vs if v["id"] == vids["筛选"])
        f["filter"][1]["value"] = 400 + i % 2 * 100 + i
        c.send("PUT", f"/api/dbs/{dbid}/views", {"views": vs, "view": vids["筛选"], "limit": 200})

    res["ui_change_filter_ms"] = measure(s, change_filter, n)
    rid = first.json()["rows"][5]["id"]
    ver = [first.json()["rows"][5]["version"]]

    def edit_cell(i):
        ver[0] = c.send("PATCH", f"/api/dbs/{dbid}/rows/{rid}", {"props": {key["数量"]["key"]: i + 1}, "version": ver[0]})["version"]

    res["ui_edit_cell_ms"] = measure(s, edit_cell, n)
    res["mcp_db_query_filter_ms"] = measure(s, lambda i: c.mcp("db_query", {"db_id": dbid, "filter": [
        {"prop": "状态", "op": "eq", "value": "进行中"}, {"prop": "单价", "op": "gt", "value": 500}]}), n)
    res["mcp_db_query_sort_ms"] = measure(s, lambda i: c.mcp("db_query", {"db_id": dbid, "sort": [{"prop": "单价", "desc": True}]}), n)
    res["mcp_db_query_formula_ms"] = measure(s, lambda i: c.mcp("db_query", {"db_id": dbid, "filter": [{"prop": "总价", "op": "gt", "value": 20000}],
                                                                             "sort": [{"prop": "总价", "desc": True}]}), n)
    res["mcp_read_page_db_ms"] = measure(s, lambda i: c.mcp("read_page", {"page_id": dbid}), max(5, n // 2))
    res["rss_after_mb"] = mb(s.rss())
    res["rss_peak_mb"] = mb(s.peak)
    res["db_file_mb"] = mb(sum(f.stat().st_size for f in data.glob("docs.db*")))
    if frontend:
        try:
            pw, browser = fe_browser()
            res["frontend_open_10k"] = fe_open(browser, s.base, f"/pages/{dbid}", 5, rows=200)
            browser.close(); pw.stop()
        except Exception as e:
            res["frontend_error"] = f"{type(e).__name__}: {e}"[:500]
            log("  frontend failed:", res["frontend_error"])
    s.stop()
    RESULTS["db"] = res
    log(f"  view p50 {res['view_table_ms']['p50']} ms, filter {res['view_filter_ms']['p50']} ms, sort {res['view_sort_ms']['p50']} ms, "
        f"formula {res['view_formula_filter_sort_ms']['p50']} ms")


def machine() -> dict:
    sh = lambda *a: subprocess.run(a, capture_output=True, text=True).stdout.strip()   # noqa: E731
    return {"cpu": sh("sysctl", "-n", "machdep.cpu.brand_string"), "cores": os.cpu_count(), "mem_gb": round(psutil.virtual_memory().total / 2**30),
            "os": f"{platform.system()} {platform.mac_ver()[0] or platform.release()}", "bench_python": platform.python_version(),
            "git": sh("git", "-C", str(REPO), "rev-parse", "--short", "HEAD"), "when": time.strftime("%Y-%m-%d %H:%M")}


def report():
    R = RESULTS
    rows = []

    def add(metric, val):
        rows.append(f"| {metric} | {val} |")

    if "coldstart" in R:
        cs = R["coldstart"]
        add("冷启动（进程 → /healthz，空目录）p50 / p95", f"{cs['fresh']['healthz_ms']['p50']} / {cs['fresh']['healthz_ms']['p95']} ms")
        add("冷启动（已有数据）p50", f"{cs['existing']['healthz_ms']['p50']} ms")
        add("空闲 RSS p50", f"{cs['fresh']['rss_idle_3s_mb']['p50']} MB")
    if "footprint" in R:
        f = R["footprint"]
        add("安装大小（venv / 静态文件）", f"{mb(f['venv_bytes'])} MB / {mb(f['static_bytes'])} MB")
    if "pages" in R:
        p = R["pages"]
        add("1000 段：网页打开三个请求 p50 / p95", f"{p['ui_open_3_requests_ms']['p50']} / {p['ui_open_3_requests_ms']['p95']} ms")
        add("1000 段：MCP read_page（首屏）/ edit_page / rewrite_page p50",
            f"{p['mcp_read_page_first_ms']['p50']} / {p['mcp_edit_page_1_block_ms']['p50']} / {p['mcp_rewrite_page_ms']['p50']} ms")
        if "frontend_open_1000" in p:
            fo = p["frontend_open_1000"]
            add("1000 段：浏览器出全部内容 冷 / 热 p50", f"{fo['cold']['full_frame']['p50']:.0f} / {fo['warm']['full_frame']['p50']:.0f} ms")
            add("1000 段：打字（keydown → 下一帧）p50 / p95", f"{p['frontend_typing_1000']['p50']} / {p['frontend_typing_1000']['p95']} ms")
    if "search" in R:
        s = R["search"]
        add("搜索 10 万段：首次（建索引）", f"{s['first_query_index_ms']} ms")
        add("搜索 10 万段：网页接口 p50 / p95", f"{s['ui_search_ms']['p50']} / {s['ui_search_ms']['p95']} ms")
        add("搜索 10 万段：MCP p50 / p95", f"{s['mcp_search_ms']['p50']} / {s['mcp_search_ms']['p95']} ms")
    if "db" in R:
        d = R["db"]
        add("数据库 1 万行：插入", f"{d['insert_total_s']} s（{d['insert_rows_per_s']} 行/秒）")
        add("数据库 1 万行：打开表格视图 / 筛选 / 排序 / 公式 p50",
            f"{d['view_table_ms']['p50']} / {d['view_filter_ms']['p50']} / {d['view_sort_ms']['p50']} / {d['view_formula_filter_sort_ms']['p50']} ms")
        if "frontend_open_10k" in d:
            add("数据库 1 万行：浏览器出 200 行 冷 / 热 p50",
                f"{d['frontend_open_10k']['cold']['full_frame']['p50']:.0f} / {d['frontend_open_10k']['warm']['full_frame']['p50']:.0f} ms")
    print("\n| 指标 | 旧版 |\n| --- | --- |\n" + "\n".join(rows))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True, help="临时数据、venv、结果放这里")
    ap.add_argument("--only", default="footprint,coldstart,pages,search,db")
    ap.add_argument("--no-frontend", action="store_true")
    ap.add_argument("--runs", type=int, default=30, help="每个接口量几次")
    ap.add_argument("--cold-runs", type=int, default=7)
    ap.add_argument("--search-pages", type=int, default=500)
    ap.add_argument("--search-blocks", type=int, default=200)
    ap.add_argument("--search-rounds", type=int, default=3)
    ap.add_argument("--db-rows", type=int, default=10000)
    a = ap.parse_args()
    out = Path(a.out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    only = set(a.only.split(","))
    if (out / "results.json").exists():   # 只重跑某几项时，别的项保留上次的结果
        RESULTS.update(json.loads((out / "results.json").read_text()))
    RESULTS["machine"] = machine()
    log("machine:", RESULTS["machine"])
    venv = venv_setup(out)
    fe = not a.no_frontend
    steps = {"footprint": lambda: bench_footprint(out, venv), "coldstart": lambda: bench_coldstart(out, venv, a.cold_runs),
             "pages": lambda: bench_pages(out, venv, a.runs, fe),
             "search": lambda: bench_search(out, venv, a.search_pages, a.search_blocks, a.search_rounds),
             "db": lambda: bench_db(out, venv, a.db_rows, max(5, a.runs // 3), fe)}
    for k, fn in steps.items():
        if k in only:
            la = [round(x, 1) for x in os.getloadavg()]
            fn()
            RESULTS[k]["loadavg_before"] = la   # 机器上别的东西忙不忙：数字偏大时先看这个
            RESULTS[k]["loadavg_after"] = [round(x, 1) for x in os.getloadavg()]
            (out / "results.json").write_text(json.dumps(RESULTS, ensure_ascii=False, indent=1))
    report()
    log(f"\nresults: {out / 'results.json'}")


if __name__ == "__main__":
    main()
