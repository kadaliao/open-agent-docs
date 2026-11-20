# embed-iframe：用 iframe 嵌整个应用

不用写前端代码：宿主页面里放一个 iframe，地址是 `https://docs.example.com/p/<页面 id>?embed=1`。
嵌入模式没有侧栏和顶栏，只有正文（或数据库），里面的链接在 iframe 里跳转。

网址参数：

| 参数 | 说明 |
| --- | --- |
| `embed=1` | 嵌入模式 |
| `theme=light` / `dark` | 主题（不给跟系统） |
| `lang=zh` / `en` | 界面语言（不给跟浏览器 / 账号；不会存下来） |

和宿主页面用 `postMessage` 说话：

| 方向 | 消息 |
| --- | --- |
| iframe → 宿主 | `{ type: 'oad:ready', pageId }` 内容出来了；`{ type: 'oad:navigate', pageId, title }` 在里面换了页；`{ type: 'oad:height', height }` 内容高度变了 |
| 宿主 → iframe | `{ type: 'oad:navigate', pageId }`；`{ type: 'oad:theme', theme: 'light' \| 'dark' \| 'system' }`；`{ type: 'oad:locale', locale: 'zh' \| 'en' }` |

登录：iframe 里是 Open Agent Docs 自己的页面，默认用它自己的登录（cookie）。宿主和它不同域名时，浏览器会拦第三方 cookie，
这时让两边同域（反向代理到同一个域名下），或者像 `jwt.html` 那样用 JWT（见下面）。

## 跑起来

```sh
OAD_AUTH=none open-agent-docs serve --port 8848      # 演示用：不登录
pnpm start                                           # http://localhost:5181/?oad=http://127.0.0.1:8848&page=<页面 id>
```

## 跨域 + JWT（jwt.html）

宿主在 `localhost:5181`，Open Agent Docs 在 `127.0.0.1:8848`（不同的站点，第三方 cookie 用不上）。宿主的后端（`serve.mjs` 的 `/token`）签 JWT，
iframe 地址写成 `…/p/<页面 id>?embed=1#token=<JWT>`；iframe 发 `oad:auth-required`（401 或快过期）时宿主再要一个，用 `oad:token` 发进去。

```sh
export OAD_JWT_SECRET='一串长的随机字符'
OAD_AUTH=jwt OAD_ALLOWED_ORIGINS=http://localhost:5181 open-agent-docs serve --port 8848
TOKEN_TTL=120 pnpm start                             # http://localhost:5181/jwt.html?oad=http://127.0.0.1:8848
```

- 不给 `&page=` 时宿主后端用这个用户建一页演示用的。`TOKEN_TTL` 是 token 几秒过期（默认 120），离过期不到 60 秒时右边能看到 iframe 来要新的。
- `OAD_ALLOWED_ORIGINS` 要写宿主的地址，不然 iframe 不认宿主发来的 `oad:token`。
- 详细的规则见[嵌入](../../docs/zh/embedding.md#跨域--jwt)。

## 冒烟测试

```sh
pnpm --filter @open-agent-docs/web build                 # 网页（嵌进来的就是它）
OAD_BIN=/path/to/open-agent-docs pnpm smoke
```

起服务器、建一页，打开宿主页面：iframe 里出正文、没有侧栏和顶栏、宿主收到 `oad:ready`；宿主发消息换成深色、英文；在 iframe 里打字能存上。
再起一个 JWT 模式的服务器，打开 `jwt.html`：跨域用 `#token=` 登录、地址栏里没有 token、快过期时 iframe 来要新的、打字能存上。
