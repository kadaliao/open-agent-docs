// 样式：编辑器、评论栏、数据库视图的 CSS 都打进包里，放进每个元素的 Shadow DOM。
// 打包时（vite.config.ts 的 cssToShadow 插件）每个 `import './x.css'` 都变成 addCss(文字)，按需加载的（公式的 katex 样式）加载时才登记，
// 已经在页面上的元素也会补上。
//
// 编辑器的样式是给整页写的（:root 上定义变量、:root[data-theme='dark'] 切深色），放进 shadow 要改成 :host：
//   :root → :host；:root[data-theme='dark'] → :host([theme='dark'])；:root:not([data-theme='light']) → :host(:not([theme='light']))
// 宿主改颜色、字体：oad-editor { --oad-accent: #7c3aed }（写在元素上，不是 :root 上——:host 的默认值会盖住继承下来的）。

const sheets: (CSSStyleSheet | string)[] = []
const roots = new Set<ShadowRoot>()
// 每个 root 已经接上了前几份
const done = new WeakMap<ShadowRoot, number>()
const canAdopt = (() => {
  try {
    return typeof CSSStyleSheet !== 'undefined' && 'replaceSync' in CSSStyleSheet.prototype && typeof document !== 'undefined' && 'adoptedStyleSheets' in document
  } catch {
    return false
  }
})()

/** 给整页写的样式改成能放进 Shadow DOM 的 */
export function toShadowCss(css: string): string {
  return css
    .replace(/:root:not\(\[data-theme=(['"]?)light\1\]\)/g, ":host(:not([theme='light']))")
    .replace(/:root\[data-theme=(['"]?)(dark|light)\1\]/g, ":host([theme='$2'])")
    .replace(/:root(?![\w-])/g, ':host')
}

// @font-face 在 Shadow DOM 里不生效（Chrome），挂到页面上（只有字体，不影响宿主的样式）
let fontStyle: HTMLStyleElement | null = null
function hoistFonts(css: string) {
  const faces = css.match(/@font-face\s*\{[^}]*\}/g)
  if (!faces || typeof document === 'undefined') return
  if (!fontStyle) {
    fontStyle = document.createElement('style')
    fontStyle.setAttribute('data-oad-fonts', '')
    document.head.append(fontStyle)
  }
  fontStyle.textContent += faces.join('\n') + '\n'
}

function attach(root: ShadowRoot, s: CSSStyleSheet | string) {
  if (typeof s === 'string') {
    const el = document.createElement('style')
    el.textContent = s
    // 放在最前面：元素自己后加的 <style>（比如 oad-viewer 的 content.css）排在后面，能盖过它
    const firstOwn = root.querySelector('style[data-oad-own], link[data-oad-own]')
    root.insertBefore(el, firstOwn ?? root.firstChild)
  } else root.adoptedStyleSheets = [...root.adoptedStyleSheets, s]
}

/** 登记一份样式（打包时自动调用）；已经在页面上的元素也补上 */
export function addCss(css: string) {
  if (!css) return
  hoistFonts(css)
  const text = toShadowCss(css)
  let s: CSSStyleSheet | string = text
  if (canAdopt) {
    s = new CSSStyleSheet()
    s.replaceSync(text)
  }
  sheets.push(s)
  roots.forEach((r) => adoptStyles(r))
}

/** 元素接到页面上时调：shadow root 补上还没有的样式，以后登记的也会跟进来 */
export function adoptStyles(root: ShadowRoot) {
  roots.add(root)
  const n = done.get(root) ?? 0
  for (let i = n; i < sheets.length; i++) attach(root, sheets[i])
  done.set(root, sheets.length)
}

/** 元素离开页面时调（不再跟进新样式，回来时 adoptStyles 再补） */
export function releaseStyles(root: ShadowRoot) {
  roots.delete(root)
}
