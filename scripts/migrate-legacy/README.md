# migrate-legacy

把旧版（Python）Open Agent Docs 的数据导进新版。用法、导什么、不导什么见 [docs/zh/migration.md](../../docs/zh/migration.md)
（[English](../../docs/en/migration.md)）。

| 文件 | |
| --- | --- |
| `migrate.mjs` | 迁移工具（Node 22.5+，用内置的 `node:sqlite` 读旧库，经 `@open-agent-docs/client` 写新版） |
| `html2md.mjs` | 旧版 HTML 页面转 Markdown |
| `fixture/docs.db` | 测试用的旧版数据（当初用旧版自己的代码造的：嵌套页面、各种块、图片、评论、Agent、分享、回收站、两个数据库） |
| `test-e2e.mjs` | 端到端测试：造数据、起新版、dry-run、导入、逐项核对、再跑一遍 |
| `test/` | 单元测试（`node --test test/`） |
