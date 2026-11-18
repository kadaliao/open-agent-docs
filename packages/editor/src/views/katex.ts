// KaTeX 按需加载：页面里第一次出现公式时才下载。没加载完先显示源码。
type Katex = typeof import('katex').default

let katex: Katex | null = null
let loading: Promise<Katex> | null = null

export function loadKatex(): Promise<Katex> {
  loading ??= Promise.all([import('katex'), import('katex/dist/katex.min.css')]).then(([m]) => {
    katex = m.default
    return katex
  })
  return loading
}

export function renderMath(el: HTMLElement, latex: string, display: boolean, empty: string) {
  if (!latex.trim()) {
    el.textContent = empty
    el.classList.add('is-empty')
    return
  }
  el.classList.remove('is-empty')
  el.dataset.latex = latex
  const draw = (k: Katex) => {
    try {
      k.render(latex, el, { displayMode: display, throwOnError: false, strict: 'ignore', output: 'htmlAndMathml' })
    } catch {
      el.textContent = latex
    }
  }
  if (katex) return draw(katex)
  el.textContent = latex
  loadKatex().then((k) => {
    // 这期间可能又改了
    if (el.dataset.latex === latex) draw(k)
  }, () => {})
}
