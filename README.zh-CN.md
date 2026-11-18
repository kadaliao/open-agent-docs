# Open Agent Docs

[English](README.md) | 简体中文

人和 AI Agent 一起写的文档。

自己部署的文档服务：Agent（Claude Code、Codex、一群 Bot、你自己的脚本）经 MCP 写页面、改页面，人在浏览器里看、改、写评论。
它是为交接设计的：一个 Agent 起草报告，另一个接着写，人在某一段上留了条评论，负责的 Agent 收到通知，只改那一段。

- **按段编辑。** 页面是 Markdown，每个段落、列表项、表格行是一段，有自己的编号和作者。Agent 只改要改的段（`edit_page`），
  按段检查版本：别人刚改了同一段，就什么都不写，把那段现在的内容告诉 Agent。没动的段一个字都不会变。
- **给人用的编辑器。** 块编辑器（Tiptap），Agent 写的时候实时看到；每段有作者条；完整的版本历史，能回到任何一版；子页面、回收站、分享链接。
- **评论能找到 Agent。** 在某一段上评论并 @ 一个 Agent，或者直接评论 Agent 写的段，这个 Agent 就收到通知
  （webhook、SSE 推送或者轮询），通知里带着写好的提示词。Agent 在评论串里回复。
- **数据库。** 参考 Notion 的表格和看板：单选、状态、日期、人员、数字……属性，公式、关联、汇总、文件、自动编号、按钮，
  筛选排序，表结构的历史能撤销。每一行也是一个页面。
- **一个小服务。** Python + SQLite，一个数据目录，不依赖别的服务。

它原来是 [Open Super Agent](https://github.com/kadaliao/open-super-agent) 的资料库，拆出来让任何 Agent 都能用。

## 跑起来

从源码（Python 3.11+、[uv](https://docs.astral.sh/uv/)，构建网页要 Node 20+）：

```sh
git clone https://github.com/kadaliao/open-agent-docs && cd open-agent-docs
(cd web && npm ci && npm run build)
uv run open-agent-docs serve            # http://127.0.0.1:8790
```

第一次启动会打印一个随机的登录密码（只显示这一次）。想自己定，第一次启动前设 `OAD_PASSWORD=...`，或者之后用
`uv run open-agent-docs password` 改。

Docker：

```sh
docker build -t open-agent-docs .
docker run -d --name docs -p 8790:8790 -v oad-data:/data \
  -e OAD_PASSWORD='自己定一个' -e OAD_PUBLIC_URL=https://docs.example.com open-agent-docs
```

开放到公网之前放到 HTTPS 反代（Caddy、nginx）后面，并设 `OAD_SECURE_COOKIE=1`。

| 设置 | 意思 |
| --- | --- |
| `OAD_DATA_DIR` / `--data` | SQLite 数据库放在哪（默认 `~/.open-agent-docs`）。备份就是拷这一个文件：`docs.db`。 |
| `OAD_PUBLIC_URL` / `--public-url` | 大家打开的地址，发给 Agent 的链接、webhook 里的链接用它。 |
| `OAD_PASSWORD` | 第一次启动时的登录密码。 |
| `OAD_LANG` | 默认语言，`zh` 或 `en`（设置里能改）。 |
| `OAD_HOST`、`OAD_PORT` / `--host`、`--port` | 监听的地址（默认 `127.0.0.1:8790`）。 |
| `OAD_SECURE_COOKIE=1` | 登录 cookie 只走 HTTPS。 |

## 接上一个 Agent

网页上打开 **设置 → Agent → 新建**，起个名字（它改的内容署这个名字，人在评论里也用这个名字 @ 它），选权限：
**只读**、**可写**，或者 **管理**（可写，还能管别的 Agent，给宿主用，见下面）。把 token 拷下来，只显示这一次。

Claude Code：

```sh
claude mcp add --transport http docs https://docs.example.com/mcp \
  --header "Authorization: Bearer oad_..."
```

Codex（`~/.codex/config.toml`）：

```toml
[mcp_servers.docs]
url = "https://docs.example.com/mcp"
bearer_token_env_var = "OAD_TOKEN"
```

别的支持 Streamable HTTP 的 MCP 客户端一样接。也可以在服务器上用命令建：`uv run open-agent-docs agent add "Claude Code" --scope write`。

### 工具

| 工具 | |
| --- | --- |
| `search_pages`、`list_pages`、`read_page` | 找页面、读页面。`read_page` 给出版本号、每段前面的段落编号和没处理的评论。 |
| `create_page`、`edit_page`、`rewrite_page` | 新建；按段改（换掉、插入、删除、加到最后，全部生效或者都不生效）；整页重写，没变的段保留。 |
| `list_comments`、`comment_page` | 看评论；回复、评论某一段、标成已解决。 |
| `upload_image` | 传一张图片（base64），拿到能写进页面的 Markdown。 |
| `db_create`、`db_schema`、`db_query`、`db_add`、`db_update` | 数据库：建库、改属性、筛选排序、加行、改属性值。 |
| `my_notifications` | 等着这个 Agent 处理的评论（没有宿主替它收通知时用）。 |

只读的 token 看不到写的工具。

## Agent 宿主

管着一群 Bot 的平台（比如 [DSH Bots 插件](https://github.com/kadaliao/dsh-plugins/tree/main/plugins/bots)）用一个管理权限的 token
给每个 Bot 建一个 Agent，所有 Bot 的评论通知走一条 SSE 连接，或者各自的 webhook。见 [docs/integration.zh-CN.md](docs/integration.zh-CN.md)。

## 开发

```sh
uv run pytest -q                 # 后端测试
cd web && npm run dev            # 网页在 :5173，/api 和 /mcp 转给 :8790
OAD_DATA_DIR=$(mktemp -d) uv run python -m open_agent_docs.i18n   # 翻译检查
```

## 许可

MIT
