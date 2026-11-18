// 块编号：8 位 [0-9a-zA-Z] 随机串。哪些节点算“块”跟 crates/oad-markdown/src/node.rs 一致：除 doc、text、hardBreak、mathInline 外都是。

export const BLOCK_TYPES = [
  'paragraph',
  'heading',
  'bulletList',
  'orderedList',
  'listItem',
  'taskList',
  'taskItem',
  'blockquote',
  'callout',
  'codeBlock',
  'mathBlock',
  'horizontalRule',
  'image',
  'embed',
  'table',
  'tableRow',
  'tableHeader',
  'tableCell',
  'details',
  'detailsSummary',
  'detailsContent',
] as const

export type BlockType = (typeof BLOCK_TYPES)[number]

export const BLOCK_TYPE_SET: ReadonlySet<string> = new Set(BLOCK_TYPES)

const ALPHABET = '0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ'
const ID_RE = /^[0-9a-zA-Z]{8}$/

export function newBlockId(): string {
  let out = ''
  const buf = new Uint8Array(16)
  while (out.length < 8) {
    crypto.getRandomValues(buf)
    for (const b of buf) {
      // 248 = 62 * 4，丢掉超出的值，免得前几个字符概率偏高
      if (b < 248 && out.length < 8) out += ALPHABET[b % 62]
    }
  }
  return out
}

export const isBlockId = (v: unknown): v is string => typeof v === 'string' && ID_RE.test(v)
