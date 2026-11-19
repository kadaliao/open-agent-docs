# migrate-legacy

把旧版（Python）Open Agent Docs 的数据导进新版。用法、导什么、不导什么见 [docs/zh/migration.md](../../docs/zh/migration.md)
（[English](../../docs/en/migration.md)）。

| 文件 | |
| --- | --- |
| `migrate.mjs` | 迁移工具（Node 22.5+，用内置的 `node:sqlite` 读旧库，经 `@oad/client` 写新版） |
| `html2md.mjs` | 旧版 HTML 页面转 Markdown |
| `make-fixture.py` | 用旧版自己的代码造一份测试数据：`cd legacy && uv run python ../scripts/migrate-legacy/make-fixture.py <目录>` |
| `test-e2e.mjs` | 端到端测试：造数据、起新版、dry-run、导入、逐项核对、再跑一遍 |
| `test/` | 单元测试（`node --test test/`） |
