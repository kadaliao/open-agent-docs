// 明亮 / 夜间：默认跟随系统，也可以在设置里固定一种。选择记在这台设备上（localStorage），不同设备各管各的。
// 页面上实际用哪种写在 <html data-theme="light|dark">，CSS 按它取颜色；index.html 里有一小段在页面画出来之前先设好，免得闪一下。
export type ThemePref = 'system' | 'light' | 'dark'
const KEY = 'oad.theme'
const system = matchMedia('(prefers-color-scheme: dark)')

export function themePref(): ThemePref {
  try { const v = localStorage.getItem(KEY); if (v === 'light' || v === 'dark') return v } catch { /* 无痕窗口 */ }
  return 'system'
}

function apply() {
  const pref = themePref()
  const theme = pref === 'system' ? (system.matches ? 'dark' : 'light') : pref
  document.querySelector('meta[name="theme-color"]')?.setAttribute('content', theme === 'dark' ? '#0d0d0e' : '#fbfbfa')
  document.documentElement.dataset.theme = theme
}

export function setThemePref(pref: ThemePref) {
  try { pref === 'system' ? localStorage.removeItem(KEY) : localStorage.setItem(KEY, pref) } catch { /* 无痕窗口 */ }
  apply()
}

export function initTheme() {
  apply()
  system.addEventListener('change', apply)
  window.addEventListener('storage', e => { if (e.key === KEY) apply() })   // 别的标签页改了
}
