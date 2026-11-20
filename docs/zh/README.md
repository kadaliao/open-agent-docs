# Open Agent Docs 文档

[English](../en/README.md)

人和 AI Agent 一起写的文档：自己部署的文档服务器，Agent 经 MCP 读写页面、回评论，人在浏览器里编辑、评论、建数据库。
一个二进制（网页打在里面），一个数据目录。

| 文档 | 内容 |
| --- | --- |
| [快速开始](quickstart.md) | 二进制、Docker、从源码编 |
| [配置](configuration.md) | 所有 `OAD_*` 环境变量和命令行参数 |
| [Agent](agents.md) | 接入 MCP（Claude Code、Codex、别的客户端）、通知、webhook 验签 |
| [Agent 看到的 Markdown](markdown.md) | 页面正文、块编号、数据库在 Markdown 里的样子 |
| [SDK](sdks.md) | Python、Go、Rust、JS 的官方 SDK：替用户调、事件、webhook、嵌入、本地起服务器，接口对照表 |
| [嵌入到自己的应用](embedding.md) | JS 包、iframe、header 登录、JWT、Node / Python 进程内启动、挂进 Rust 服务 |
| [HTTP 接口](api.md) | OpenAPI、登录、错误码、翻页、限流、版本、实时事件 |
| [部署](deployment.md) | 反向代理、HTTPS、备份 |
| [从旧版迁移](migration.md) | 把 Python 版的数据导进来 |

接口参考：服务器的 `GET /api/v1/openapi.json`（仓库里也有一份 [openapi.json](../openapi.json)）。
