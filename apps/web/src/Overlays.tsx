// 全局快捷键和浮在最上面的面板（⌘K 搜索、⌘/ 快捷键帮助）。面板的代码单独一个包，第一次打开（或空下来时）才下载。
import { useEffect, useState } from 'react'
import type { Me } from './api'
import { onCommand, runCommand, toggleTheme } from './lib'
import type { PageStore } from './store'

type PaletteModule = typeof import('./palette/Palette')
let mod: PaletteModule | null = null
let loading: Promise<PaletteModule> | null = null
const load = () => (loading ??= import('./palette/Palette').then((m) => (mod = m)))

const editable = (el: EventTarget | null) => {
  const e = el as HTMLElement | null
  return !!e && (e.isContentEditable || e.tagName === 'INPUT' || e.tagName === 'TEXTAREA' || e.tagName === 'SELECT')
}

export function Overlays({ store, me, pageId }: { store: PageStore; me: Me; pageId: string | null }) {
  const [open, setOpen] = useState<null | 'palette' | 'shortcuts'>(null)
  const [, force] = useState(0)
  useEffect(() => {
    if (open && !mod) load().then(() => force((x) => x + 1))
  }, [open])
  // 空下来时先下载好，第一次按 ⌘K 不等
  useEffect(() => {
    const id = (window.requestIdleCallback ?? ((f: () => void) => setTimeout(f, 1500)))(() => load())
    return () => (window.cancelIdleCallback ?? clearTimeout)(id as number)
  }, [])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const m = e.metaKey || e.ctrlKey
      if (!m || e.isComposing) return
      const k = e.key.toLowerCase()
      // 编辑器先处理（⌘K 选中文字时是加链接）；处理了就不管
      if (e.defaultPrevented) return
      if ((k === 'k' || k === 'p') && !e.shiftKey && !e.altKey) {
        e.preventDefault()
        setOpen((o) => (o === 'palette' ? null : 'palette'))
      } else if (k === '/' && !e.altKey) {
        e.preventDefault()
        setOpen((o) => (o === 'shortcuts' ? null : 'shortcuts'))
      } else if (k === 'l' && e.shiftKey) {
        e.preventDefault()
        toggleTheme()
      } else if (k === '\\') {
        e.preventDefault()
        runCommand('sidebar')
      } else if ((k === '[' || k === ']') && !e.shiftKey && !editable(e.target)) {
        e.preventDefault()
        if (k === '[') history.back()
        else history.forward()
      }
    }
    window.addEventListener('keydown', onKey)
    const off = onCommand((c) => {
      if (c === 'palette' || c === 'shortcuts') setOpen(c)
    })
    return () => {
      window.removeEventListener('keydown', onKey)
      off()
    }
  }, [])

  if (!open || !mod) return null
  return open === 'palette' ? (
    <mod.Palette store={store} me={me} pageId={pageId} onClose={() => setOpen(null)} />
  ) : (
    <mod.ShortcutHelp onClose={() => setOpen(null)} />
  )
}
