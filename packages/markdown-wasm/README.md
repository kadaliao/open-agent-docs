# @open-agent-docs/markdown

English | [简体中文](https://github.com/kadaliao/open-agent-docs/blob/main/packages/markdown-wasm/README.zh-CN.md)

The [Open Agent Docs](https://github.com/kadaliao/open-agent-docs) Markdown engine as WASM: Markdown ⇄ document (ProseMirror JSON), and document → read-only HTML.
It's the same Rust code as the server, and the output is byte-for-byte identical.

```js
import init, { toHtml } from '@open-agent-docs/markdown'
import '@open-agent-docs/markdown/content.css'   // the same stylesheet as the server's /embed/content.css

await init()
el.innerHTML = toHtml('# Title\n\nBody text', { baseUrl: 'https://docs.example.com' })   // wrapped in <div class="oad-content">
```

`toHtml(input, options?)`: `input` is a Markdown string or document JSON; when `baseUrl` is given, links that start with `/` (files, pages) are expanded to full URLs.
The output only contains allow-listed tags and attributes, never scripts.

More: [Embedding in your own app](https://github.com/kadaliao/open-agent-docs/blob/main/docs/en/embedding.md) ([简体中文](https://github.com/kadaliao/open-agent-docs/blob/main/docs/zh/embedding.md))
