// Shiki 按需加载：引擎和主题第一次用到代码块时才下载，每种语言单独一个小包。只收常用的几十种，免得打进二进制的网页太大。
import type { HighlighterCore } from 'shiki/core'

const LANGS: Record<string, () => Promise<unknown>> = {
  bash: () => import('shiki/langs/bash.mjs'),
  c: () => import('shiki/langs/c.mjs'),
  cpp: () => import('shiki/langs/cpp.mjs'),
  csharp: () => import('shiki/langs/csharp.mjs'),
  css: () => import('shiki/langs/css.mjs'),
  dart: () => import('shiki/langs/dart.mjs'),
  diff: () => import('shiki/langs/diff.mjs'),
  docker: () => import('shiki/langs/docker.mjs'),
  elixir: () => import('shiki/langs/elixir.mjs'),
  go: () => import('shiki/langs/go.mjs'),
  graphql: () => import('shiki/langs/graphql.mjs'),
  haskell: () => import('shiki/langs/haskell.mjs'),
  html: () => import('shiki/langs/html.mjs'),
  ini: () => import('shiki/langs/ini.mjs'),
  java: () => import('shiki/langs/java.mjs'),
  javascript: () => import('shiki/langs/javascript.mjs'),
  json: () => import('shiki/langs/json.mjs'),
  jsx: () => import('shiki/langs/jsx.mjs'),
  kotlin: () => import('shiki/langs/kotlin.mjs'),
  latex: () => import('shiki/langs/latex.mjs'),
  lua: () => import('shiki/langs/lua.mjs'),
  make: () => import('shiki/langs/make.mjs'),
  markdown: () => import('shiki/langs/markdown.mjs'),
  nginx: () => import('shiki/langs/nginx.mjs'),
  objc: () => import('shiki/langs/objective-c.mjs'),
  php: () => import('shiki/langs/php.mjs'),
  powershell: () => import('shiki/langs/powershell.mjs'),
  protobuf: () => import('shiki/langs/proto.mjs'),
  python: () => import('shiki/langs/python.mjs'),
  r: () => import('shiki/langs/r.mjs'),
  ruby: () => import('shiki/langs/ruby.mjs'),
  rust: () => import('shiki/langs/rust.mjs'),
  scala: () => import('shiki/langs/scala.mjs'),
  scss: () => import('shiki/langs/scss.mjs'),
  sql: () => import('shiki/langs/sql.mjs'),
  swift: () => import('shiki/langs/swift.mjs'),
  toml: () => import('shiki/langs/toml.mjs'),
  tsx: () => import('shiki/langs/tsx.mjs'),
  typescript: () => import('shiki/langs/typescript.mjs'),
  vue: () => import('shiki/langs/vue.mjs'),
  xml: () => import('shiki/langs/xml.mjs'),
  yaml: () => import('shiki/langs/yaml.mjs'),
  zig: () => import('shiki/langs/zig.mjs'),
}

const ALIAS: Record<string, string> = {
  sh: 'bash',
  shell: 'bash',
  zsh: 'bash',
  console: 'bash',
  js: 'javascript',
  mjs: 'javascript',
  cjs: 'javascript',
  ts: 'typescript',
  mts: 'typescript',
  py: 'python',
  rs: 'rust',
  rb: 'ruby',
  'c++': 'cpp',
  cc: 'cpp',
  h: 'c',
  cs: 'csharp',
  'c#': 'csharp',
  kt: 'kotlin',
  yml: 'yaml',
  md: 'markdown',
  dockerfile: 'docker',
  golang: 'go',
  tex: 'latex',
  proto: 'protobuf',
  ps1: 'powershell',
  makefile: 'make',
  'objective-c': 'objc',
  svg: 'xml',
}

/** 语言选择里列出来的 */
export const LANGUAGE_NAMES = Object.keys(LANGS)

export function normalizeLang(l: string | null | undefined): string | null {
  if (!l) return null
  const k = l.trim().toLowerCase()
  const name = ALIAS[k] ?? k
  return LANGS[name] ? name : null
}

let hl: Promise<HighlighterCore> | null = null
const loaded = new Set<string>()
const loading = new Map<string, Promise<void>>()

function highlighter() {
  hl ??= (async () => {
    const [{ createHighlighterCore }, { createJavaScriptRegexEngine }, light, dark] = await Promise.all([
      import('shiki/core'),
      import('shiki/engine/javascript'),
      import('shiki/themes/github-light.mjs'),
      import('shiki/themes/github-dark.mjs'),
    ])
    return createHighlighterCore({ themes: [light.default, dark.default], langs: [], engine: createJavaScriptRegexEngine() })
  })()
  return hl
}

export async function ensureLang(lang: string) {
  const h = await highlighter()
  if (loaded.has(lang)) return h
  let p = loading.get(lang)
  if (!p) {
    p = (LANGS[lang]() as Promise<{ default: any }>).then((m) => h.loadLanguage(m.default)).then(() => {
      loaded.add(lang)
    })
    loading.set(lang, p)
  }
  await p
  return h
}

export interface Token {
  from: number
  to: number
  style: string
}

/** 一段代码 → 每个片段的颜色（明暗两套，写成 CSS 变量） */
export async function tokenize(code: string, lang: string): Promise<Token[]> {
  const h = await ensureLang(lang)
  const res = h.codeToTokens(code, { lang, themes: { light: 'github-light', dark: 'github-dark' }, defaultColor: false })
  const out: Token[] = []
  let off = 0
  res.tokens.forEach((line, i) => {
    for (const tk of line) {
      const len = tk.content.length
      const st = tk.htmlStyle
      if (st && len) {
        const style = Object.entries(st)
          .map(([k, v]) => `${k}:${v}`)
          .join(';')
        out.push({ from: off, to: off + len, style })
      }
      off += len
    }
    if (i < res.tokens.length - 1) off += 1 // 换行
  })
  return out
}
