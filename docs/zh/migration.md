# 从旧版迁移

[English](../en/migration.md) · [文档目录](README.md)

旧版（Python，`uv run open-agent-docs`，数据在 `~/.open-agent-docs/docs.db`）和新版的数据格式不一样，不能直接换程序。
用 `scripts/migrate-legacy` 把旧数据经新版的接口导进去：旧版不用开着，只读它的数据库文件，不会改它。

## 步骤

1. 备份旧版的数据目录（`docs.db` 一个文件）。
2. 启动新版（[快速开始](quickstart.md)），建好管理员。
3. 在仓库里准备好迁移工具（要 Node 22.5 以上）：
   ```sh
   pnpm install && pnpm --filter @open-agent-docs/client build
   ```
4. 先看看会导什么（不写任何东西）：
   ```sh
   node scripts/migrate-legacy/migrate.mjs --from ~/.open-agent-docs --to http://127.0.0.1:8848 \
     --token "$OAD_TOKEN" --dry-run
   ```
5. 真的导：
   ```sh
   node scripts/migrate-legacy/migrate.mjs --from ~/.open-agent-docs --to http://127.0.0.1:8848 \
     --token "$OAD_TOKEN" --tokens-out ./agent-tokens.txt
   ```
6. 看报告 `oad-migration-report.md`，把 `agent-tokens.txt` 里的新 token 换到各个 Agent 的配置里，然后删掉这个文件。

登录：`--token` 给一个新版的会话令牌，或者 `admin` 权限的 Agent token（在新版网页上建一个）。
新版用 `OAD_AUTH=none` 跑着时不用给；`header` 登录时用 `--header x-oad-user-id:me --header x-oad-secret:…`。
导进去的内容以这个身份写入；旧版人员属性里的“我”换成建这个 token 的人（找不到就用第一个所有者）。

## 参数

| 参数 | 说明 |
| --- | --- |
| `--from` | 旧版的数据目录，或者 `docs.db` 文件 |
| `--to` | 新版的地址，默认 `http://127.0.0.1:8848`（或环境变量 `OAD_URL`） |
| `--token` | 新版的 token（或环境变量 `OAD_TOKEN`） |
| `--header k:v` | 每个请求多带的头，可以给多次 |
| `--workspace` | 导进哪个工作区，默认第一个 |
| `--map` | 对照表（旧编号 → 新编号），默认 `./oad-migration-map.json` |
| `--report` | 报告，默认 `./oad-migration-report.md` |
| `--tokens-out` | 重建的 Agent 的新 token 写到这里（权限 600）。不给就不建 Agent |
| `--dry-run` | 只看会导什么，不写 |
| `--no-agents`、`--no-comments`、`--no-shares`、`--no-trash` | 这几样不导 |

## 导什么

| 旧版 | 新版 |
| --- | --- |
| 页面、子页面、顺序 | 一样的树。Markdown 正文原样导入；HTML 页面转成 Markdown（只认常见的标签） |
| 图片和文件 | 重新上传，正文和文件属性里的地址换成新的 |
| 数据库 | 表结构（选项和颜色、状态分组、数字格式、自动编号前缀）、行和行的正文、视图（排序、筛选、隐藏的列、看板分组） |
| 关联、汇总、公式、按钮 | 关联（双向的也是）换成新的行编号；汇总、公式、按钮重新建。公式在新版里写不通的，留一列文字并在报告里列出来 |
| 评论 | 成串导入，回复、已解决照旧。都挂在整页上，原来引用的那段文字放在评论开头；Agent 写的评论开头写着原来的作者 |
| Agent | 重新建（名字、说明、权限、webhook），**新 token** 只写进 `--tokens-out` 的文件，不在屏幕上打出来。撤销了的不导 |
| 公开分享 | 重新生成链接，新旧对照写在报告里（旧链接失效） |
| 回收站 | 普通页面导进去再删，留在新版的回收站里 |

不导的：页面的历史版本（新版从导入的这一版开始记）、置顶。创建时间、编辑时间、创建人按导入的时候算；自动编号按导入的顺序重新发。
这些都写在报告的“和旧版不一样的地方”里。

## 再跑一次

对照表记着导过的每一样东西。再跑一次只补没导过的，什么都不会重复。中途断了（网络、新版重启）就再跑一遍。
想重新导一次：删掉新版的数据和对照表。

## 测试

```sh
OAD_BIN=target/release/open-agent-docs node scripts/migrate-legacy/test-e2e.mjs
```

用旧版自己的代码（`cd legacy && uv run …`）造一份覆盖各种情况的旧数据，起一个新版服务器，先 `--dry-run`（确认什么都没写），
再导一遍逐项核对，再跑一遍确认什么都没多。
