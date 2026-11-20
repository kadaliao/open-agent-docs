// packages/markdown-wasm（@open-agent-docs/markdown，服务器那边编 WASM）还没有的时候用这个占位：不解析，粘贴的 Markdown 当纯文本。
export const available = false
export function parse(): null {
  return null
}
