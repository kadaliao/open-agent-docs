"""Open Agent Docs 网页的端到端检查：登录、写页面、Agent 经 MCP 改、评论通知、数据库、分享、英文、手机、断线重连。

先构建网页（cd web && npm run build），再在仓库根目录跑：
  uv run --with playwright python tests/browser/e2e.py [http://127.0.0.1:8793]
脚本自己在临时数据目录上起服务、跑完停掉；截图放在 OAD_E2E_SHOTS（默认临时目录，结束时打印出来）。
"""
import json
import re
import sys
import time
import traceback
import os
import tempfile
from pathlib import Path
import subprocess
import urllib.request

from playwright.sync_api import sync_playwright, expect

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8793"
SHOTS = os.environ.get("OAD_E2E_SHOTS") or tempfile.mkdtemp(prefix="oad-shots-")
PW = "test-password-1"
results = []
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def step(name):
    def deco(fn):
        def run(*a, **k):
            try:
                out = fn(*a, **k)
                results.append((name, "PASS", ""))
                print("PASS", name, flush=True)
                return out
            except Exception as e:
                results.append((name, "FAIL", f"{type(e).__name__}: {e}"[:400]))
                print("FAIL", name, e, flush=True)
                traceback.print_exc()
        return run
    return deco


def mcp(token, method, params=None, sid=None, notify=False):
    body = {"jsonrpc": "2.0", "method": method, **({"params": params} if params is not None else {})}
    if not notify:
        body["id"] = int(time.time() * 1000) % 100000
    req = urllib.request.Request(BASE + "/mcp", data=json.dumps(body).encode(), method="POST", headers={
        "authorization": f"Bearer {token}", "content-type": "application/json",
        "accept": "application/json, text/event-stream", **({"mcp-session-id": sid} if sid else {})})
    with opener.open(req, timeout=20) as r:
        sid = r.headers.get("mcp-session-id") or sid
        raw = r.read().decode()
    if notify:
        return None, sid
    if raw.lstrip().startswith("{"):
        return json.loads(raw), sid
    datas = [l[5:].strip() for l in raw.splitlines() if l.startswith("data:")]
    return json.loads(datas[-1]), sid


def mcp_tool(token, name, args):
    init, sid = mcp(token, "initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "e2e", "version": "0"}})
    mcp(token, "notifications/initialized", sid=sid, notify=True)
    res, _ = mcp(token, "tools/call", {"name": name, "arguments": args}, sid=sid)
    return res


st = {}
DATA = tempfile.mkdtemp(prefix="oad-e2e-")
REPO = str(Path(__file__).resolve().parents[2])
PORT = BASE.rsplit(":", 1)[1]
server = None


def start_server():
    global server
    env = {**os.environ, "OAD_DATA_DIR": DATA, "OAD_PASSWORD": PW}
    server = subprocess.Popen(["uv", "run", "open-agent-docs", "serve", "--port", PORT], cwd=REPO, env=env,
                              stdout=open(DATA + "/server.log", "a"), stderr=subprocess.STDOUT)
    for _ in range(60):
        try:
            opener.open(BASE + "/healthz", timeout=1).read()
            return
        except Exception:
            time.sleep(0.5)
    raise RuntimeError("server did not start")


def stop_server():
    server.terminate()
    try:
        server.wait(5)
    except subprocess.TimeoutExpired:   # 开着的 SSE 连接让 uvicorn 一直等着优雅退出：直接杀掉
        print("server did not exit on SIGTERM within 5s (open SSE connections); killing", flush=True)
        subprocess.run(["pkill", "-f", f"open-agent-docs serve --port {PORT}"])
        server.kill()
        server.wait(5)


start_server()

with sync_playwright() as p:
    browser = p.chromium.launch(args=["--no-proxy-server"])
    ctx = browser.new_context(viewport={"width": 1320, "height": 860}, locale="zh-CN")
    page = ctx.new_page()
    errors = []
    page.on("pageerror", lambda e: errors.append(str(e)))
    page.on("console", lambda m: errors.append("console: " + m.text) if m.type == "error" else None)

    @step("1 登录页：错密码报错，对的密码进去")
    def login():
        page.goto(BASE + "/")
        page.wait_for_selector(".login-box input[type=password]")
        page.screenshot(path=f"{SHOTS}/01-login.png")
        page.fill(".login-box input[type=password]", "wrong-password")
        page.click(".login-box button[type=submit]")
        expect(page.locator(".login-box .error-line")).to_contain_text("密码不对", timeout=5000)
        page.fill(".login-box input[type=password]", PW)
        page.click(".login-box button[type=submit]")
        page.wait_for_selector(".sidebar", timeout=8000)
        expect(page.locator(".home-title")).to_have_text("Open Agent Docs")
        page.screenshot(path=f"{SHOTS}/02-home-empty.png")
    login()

    @step("2 新建页面、改标题、写段落 / 列表 / 表格")
    def write():
        page.click(".side-new .btn >> nth=0")
        page.wait_for_url(re.compile(r"/pages/\d+$"), timeout=8000)
        st["pid"] = int(page.url.rsplit("/", 1)[1])
        page.wait_for_selector(".doc-body.ProseMirror", timeout=10000)
        page.click(".page-title")
        page.fill(".page-title-input", "测试页面")
        page.keyboard.press("Enter")
        expect(page.locator(".page-title")).to_have_text("测试页面")
        ed = page.locator(".doc-body.ProseMirror")
        ed.click()
        page.keyboard.type("第一段：人和 Agent 一起写的文档。")
        page.keyboard.press("Enter")
        page.keyboard.type("第二段：按段保存，每段记着作者。")
        page.keyboard.press("Enter")
        page.keyboard.type("- 列表一")
        page.keyboard.press("Enter")
        page.keyboard.type("列表二")
        page.keyboard.press("Enter")
        page.keyboard.press("Enter")
        page.keyboard.type("/表格")
        page.wait_for_selector(".slash-menu", timeout=3000)
        page.keyboard.press("Enter")
        page.wait_for_selector(".doc-body table", timeout=3000)
        page.keyboard.type("名字")
        page.keyboard.press("Tab")
        page.keyboard.type("状态")
        page.keyboard.press("Tab")
        page.keyboard.type("备注")
        page.keyboard.press("Tab")
        page.keyboard.type("甲")
        expect(page.locator(".doc-status")).to_contain_text("已保存", timeout=8000)
        time.sleep(1.5)
        expect(page.locator(".doc-status")).to_contain_text("已保存", timeout=8000)
        page.screenshot(path=f"{SHOTS}/03-doc-typed.png")
    write()

    @step("3 刷新以后内容还在（段落、列表、表格）")
    def persisted():
        page.reload()
        page.wait_for_selector(".doc-body.ProseMirror", timeout=10000)
        body = page.locator(".doc-body")
        expect(body).to_contain_text("第一段：人和 Agent 一起写的文档。", timeout=8000)
        expect(body).to_contain_text("第二段：按段保存")
        assert body.locator("li").count() >= 2, "列表项不够"
        assert body.locator("table").count() == 1, "没有表格"
        expect(body.locator("table")).to_contain_text("甲")
        md = page.request.get(f"{BASE}/api/pages/{st['pid']}").json()["content"]
        assert "- 列表一" in md and "| 名字" in md, md
        st["md_before"] = md
    persisted()

    @step("4 设置里新建 Agent，显示 token、webhook 密钥和接法")
    def make_agent():
        page.click(".side-bottom .side-link >> nth=1")
        page.wait_for_url(re.compile(r"/settings$"))
        page.fill(".agent-new input >> nth=0", "Claude Code")
        page.fill(".agent-new input >> nth=1", "https://example.com/hook")
        page.click(".agent-new .btn.primary")
        page.wait_for_selector(".agent-made", timeout=5000)
        pres = page.locator(".agent-made .agent-snippet pre")
        st["token"] = pres.nth(0).inner_text().strip()
        assert st["token"].startswith("oad_"), st["token"]
        texts = [pres.nth(i).inner_text() for i in range(pres.count())]
        assert any(t.startswith("claude mcp add --transport http docs ") and "/mcp" in t and st["token"] in t for t in texts), texts
        assert any("bearer_token_env_var" in t for t in texts)
        assert any('"mcpServers"' in t for t in texts)
        page.screenshot(path=f"{SHOTS}/04-agent-created.png", full_page=True)
        page.click(".agent-made .btn.primary")
        expect(page.locator(".agent-item")).to_have_count(1)
        page.click(".agent-item .ghost-btn >> nth=0")   # 设置
        page.click(".agent-edit .btn >> text=看 webhook 密钥")
        page.wait_for_selector(".agent-edit .agent-snippet pre")
        page.screenshot(path=f"{SHOTS}/05-settings.png", full_page=True)
    make_agent()

    @step("5 Agent 经 MCP edit_page 改页面：打开着的编辑器实时更新，作者显示 Agent")
    def agent_edit():
        page.goto(f"{BASE}/pages/{st['pid']}")
        page.wait_for_selector(".doc-body.ProseMirror", timeout=10000)
        expect(page.locator(".doc-status")).to_contain_text("已保存")
        time.sleep(1)
        v = page.request.get(f"{BASE}/api/pages/{st['pid']}/blocks").json()["version"]
        res = mcp_tool(st["token"], "edit_page", {"page_id": st["pid"], "version": v,
                                                  "ops": [{"op": "append", "markdown": "这一段是 Agent 经 MCP 加的。"}]})
        assert "error" not in res and not res["result"].get("isError"), res
        blk = page.locator(".doc-body [data-by='Claude Code']")
        expect(blk).to_contain_text("这一段是 Agent 经 MCP 加的。", timeout=10000)
        st["agent_block_title"] = blk.first.get_attribute("title")
        page.screenshot(path=f"{SHOTS}/06-agent-edit-live.png")
    agent_edit()

    @step("6 在 Agent 写的段上评论 @Claude Code：返回的 woke 有它，面板显示“通知了”")
    def comment():
        blk = page.locator(".doc-body [data-by='Claude Code']").first
        blk.click(click_count=3)
        page.wait_for_selector(".doc-toolbar button[title='评论']", timeout=3000)
        page.click(".doc-toolbar button[title='评论']")
        page.wait_for_selector(".doc-comments .dc-new textarea")
        page.locator(".doc-comments .dc-new textarea").type("@Claude")
        expect(page.locator(".dc-mentions")).to_contain_text("@Claude Code")
        page.keyboard.press("Enter")   # 选中提示里的名字
        page.locator(".doc-comments .dc-new textarea").type("这一段再写详细一点")
        with page.expect_response(lambda r: r.url.endswith(f"/api/pages/{st['pid']}/comments") and r.request.method == "POST") as resp:
            page.click(".doc-comments .dc-new .btn.primary")
        c = resp.value.json()
        assert c["woke"] and c["block_id"], c
        st["woke"] = c["woke"]
        expect(page.locator(".dc-woke")).to_contain_text("通知了 Claude Code", timeout=5000)
        expect(page.locator(".dc-pending")).to_contain_text("等 Claude Code 回复")
        # Agent 回复评论（不叫醒任何人）
        res = mcp_tool(st["token"], "comment_page", {"page_id": st["pid"], "reply_to": c["id"], "text": "好的，我来改。"})
        assert not res["result"].get("isError"), res
        expect(page.locator(".doc-comments")).to_contain_text("好的，我来改。", timeout=8000)
        page.screenshot(path=f"{SHOTS}/07-comments.png")
        page.click(".doc-comments .dc-head .icon-btn")
    comment()

    @step("7 新建数据库、加三行、切到看板、打开一行（属性栏）")
    def database():
        before = page.url
        page.click(".side-new .btn >> nth=1")
        page.wait_for_function("u => location.href !== u && /\\/pages\\/\\d+$/.test(location.pathname)", arg=before, timeout=8000)
        st["dbid"] = int(page.url.rsplit("/", 1)[1])
        page.wait_for_selector(".db-table", timeout=8000)
        for name in ["写需求", "做原型", "上线"]:
            page.click(".db-new")
            inp = page.locator(".db-table input:focus, .db-table textarea:focus")
            inp.wait_for(timeout=3000)
            page.keyboard.type(name)
            page.keyboard.press("Enter")
            expect(page.locator(".db-table")).to_contain_text(name, timeout=5000)
        page.screenshot(path=f"{SHOTS}/08-db-table.png")
        page.click(".db-tab >> text=看板")
        page.wait_for_selector(".db-card", timeout=5000)
        expect(page.locator(".db-card")).to_have_count(3)
        page.screenshot(path=f"{SHOTS}/09-db-board.png")
        page.locator(".db-card", has_text="做原型").click()
        page.wait_for_url(re.compile(r"/pages/\d+$"))
        page.wait_for_selector(".db-props", timeout=8000)
        page.wait_for_selector(".doc-body.ProseMirror", timeout=10000)
        expect(page.locator(".page-title")).to_have_text("做原型")
        page.locator(".doc-body.ProseMirror").click()
        page.keyboard.type("行页面的正文。")
        expect(page.locator(".doc-status")).to_contain_text("已保存", timeout=8000)
        page.screenshot(path=f"{SHOTS}/10-db-row-page.png")
    database()

    @step("8 分享页面，退出登录的浏览器打开 /s/<token> 只读看到内容")
    def share():
        page.goto(f"{BASE}/pages/{st['pid']}")
        page.wait_for_selector(".page-actions")
        page.click(".page-actions .btn >> text=分享")
        expect(page.locator(".page-actions")).to_contain_text("取消分享", timeout=5000)
        token = page.request.get(f"{BASE}/api/pages/{st['pid']}").json()["share_token"]
        anon = browser.new_context(viewport={"width": 1100, "height": 800})
        sp = anon.new_page()
        sp.goto(f"{BASE}/s/{token}")
        expect(sp.locator(".shared-head")).to_contain_text("测试页面", timeout=8000)
        expect(sp.locator(".page-md")).to_contain_text("这一段是 Agent 经 MCP 加的。")
        assert sp.locator(".page-md table").count() == 1
        assert sp.locator(".ProseMirror, .sidebar, .login-box").count() == 0
        sp.screenshot(path=f"{SHOTS}/11-shared.png")
        r = anon.request.get(f"{BASE}/api/pages/{st['pid']}")
        assert r.status == 401, r.status
        anon.close()
    share()

    @step("9 首页、侧栏（置顶、树、搜索）")
    def home():
        page.goto(f"{BASE}/pages/{st['pid']}")
        page.wait_for_selector(".page-more")
        page.click(".page-more > .btn")
        page.click(".page-more .menu button >> text=置顶")
        expect(page.locator(".side-group").first).to_have_text("置顶", timeout=5000)
        page.click(".logo-btn")
        page.wait_for_url(BASE + "/")
        expect(page.locator(".home .lib-row")).to_have_count(2)
        page.fill(".side-search input", "Agent")
        expect(page.locator(".side-hit").first).to_be_visible(timeout=5000)
        page.screenshot(path=f"{SHOTS}/12-home-search.png")
        page.fill(".side-search input", "")
    home()

    @step("10 切到英文：界面文字换成英文")
    def english():
        page.goto(f"{BASE}/settings")
        page.wait_for_selector(".seg")
        page.click(".seg button >> text=English")
        page.wait_for_load_state("load")
        expect(page.locator(".side-bottom")).to_contain_text("Settings", timeout=8000)
        expect(page.locator(".head-name")).to_have_text("Settings")
        page.screenshot(path=f"{SHOTS}/13-settings-en.png", full_page=True)
        page.goto(f"{BASE}/pages/{st['pid']}")
        page.wait_for_selector(".doc-body.ProseMirror", timeout=10000)
        expect(page.locator(".doc-status")).to_contain_text("Saved")
        page.screenshot(path=f"{SHOTS}/14-doc-en.png")
        page.request.put(f"{BASE}/api/settings", data=json.dumps({"language": "zh"}), headers={"content-type": "application/json"})
    english()

    state = ctx.storage_state()

    @step("11 手机（390 宽）：侧栏收起成抽屉，编辑器能打字，数据库能看")
    def phone():
        ph = browser.new_context(viewport={"width": 390, "height": 844}, storage_state=state, has_touch=True, is_mobile=True,
                                 device_scale_factor=2, locale="zh-CN")
        m = ph.new_page()
        m.add_init_script("try{localStorage.setItem('oad.lang','zh')}catch(e){}")
        m.goto(f"{BASE}/pages/{st['pid']}")
        m.wait_for_selector(".doc-body.ProseMirror", timeout=10000)
        assert not m.locator(".sidebar").is_visible() or m.locator(".sidebar").bounding_box()["x"] < -100, "侧栏没收起"
        m.screenshot(path=f"{SHOTS}/15-phone-doc.png")
        m.locator(".doc-body p").first.tap()
        m.keyboard.press("End")
        m.keyboard.type("（手机上加的）")
        expect(m.locator(".doc-status")).to_contain_text("已保存", timeout=8000)
        m.tap(".mobile-menu")
        time.sleep(0.4)
        assert m.locator(".sidebar").bounding_box()["x"] >= 0
        m.screenshot(path=f"{SHOTS}/16-phone-drawer.png")
        m.tap(".side-mask", position={"x": 370, "y": 420})
        time.sleep(0.3)
        m.click(".doc-cbtn >> nth=1")
        m.wait_for_selector(".doc-comments")
        m.screenshot(path=f"{SHOTS}/17-phone-comments.png")
        m.goto(f"{BASE}/pages/{st['dbid']}")
        try:
            m.wait_for_selector(".db-card, .db-table", timeout=8000)
        except Exception:
            m.screenshot(path=f"{SHOTS}/fail-phone-db.png")
            print("URL", m.url, m.locator("main").inner_text()[:500])
            raise
        m.screenshot(path=f"{SHOTS}/18-phone-db.png")
        m.goto(f"{BASE}/settings")
        m.wait_for_selector(".agent-item")
        m.screenshot(path=f"{SHOTS}/19-phone-settings.png", full_page=True)
        m.goto(f"{BASE}/")
        m.wait_for_selector(".home-title")
        m.screenshot(path=f"{SHOTS}/20-phone-home.png")
        md = m.request.get(f"{BASE}/api/pages/{st['pid']}").json()["content"]
        assert "（手机上加的）" in md, md
        ph.close()
    phone()

    @step("12 没改过的段一个字都没动（Agent 加段、评论之后正文前面部分不变）")
    def untouched():
        md = page.request.get(f"{BASE}/api/pages/{st['pid']}").json()["content"]
        before = st["md_before"].split("\n\n")
        after = md.split("\n\n")
        assert after[1:len(before)] == before[1:], (before, after)
    untouched()

    @step("13 服务器重启（断线重连）以后：连接灯恢复，Agent 的新改动照样实时出现")
    def reconnect():
        page.goto(f"{BASE}/pages/{st['pid']}")
        page.wait_for_selector(".doc-body.ProseMirror", timeout=10000)
        expect(page.locator(".conn.on")).to_have_count(1, timeout=5000)
        stop_server()
        expect(page.locator(".conn.on")).to_have_count(0, timeout=10000)
        start_server()
        expect(page.locator(".conn.on")).to_have_count(1, timeout=15000)
        v = page.request.get(f"{BASE}/api/pages/{st['pid']}/blocks").json()["version"]
        res = mcp_tool(st["token"], "edit_page", {"page_id": st["pid"], "version": v,
                                                  "ops": [{"op": "append", "markdown": "重连以后 Agent 又加了一段。"}]})
        assert not res["result"].get("isError"), res
        expect(page.locator(".doc-body")).to_contain_text("重连以后 Agent 又加了一段。", timeout=10000)
    reconnect()

    print("\nJS errors:", [e for e in errors if "favicon" not in e][:10])
    browser.close()

stop_server()
print("\n== RESULTS ==")
for n, s, d in results:
    print(f"{s}  {n}  {d}")
print(f"\nscreenshots: {SHOTS}")
