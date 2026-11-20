# @open-agent-docs/markdown

[Open Agent Docs](https://github.com/kadaliao/open-agent-docs) 的 Markdown 引擎（WASM）：Markdown 和文档（ProseMirror JSON）互转，文档转成只读 HTML。
和服务器是同一份 Rust 代码，输出逐字一样。

The Open Agent Docs Markdown engine as WASM: Markdown ⇄ document JSON, and document → read-only HTML, byte-for-byte identical to the server.

```js
import init, { toHtml } from '@open-agent-docs/markdown'
import '@open-agent-docs/markdown/content.css'   // 和服务器 /embed/content.css 同一份样式

await init()
el.innerHTML = toHtml('# 标题\n\n正文', { baseUrl: 'https://docs.example.com' })   // 外面包着 <div class="oad-content">
```

`toHtml(input, options?)`：`input` 是 Markdown 字符串或文档 JSON；`baseUrl` 给了时，以 `/` 开头的链接（文件、页面）拼成完整地址。
输出只有白名单里的标签和属性，不会有脚本。

更多：[嵌入到自己的应用](https://github.com/kadaliao/open-agent-docs/blob/main/docs/zh/embedding.md) · [English](https://github.com/kadaliao/open-agent-docs/blob/main/docs/en/embedding.md)
