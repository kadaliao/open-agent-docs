// 打开正文里的链接：点一下就打开（编辑时也是）。要改链接里的字：拖着选、按住 ⌥ 或 ⇧ 点只放光标 / 扩选区，
// 光标停在链接里会弹出链接卡片（ui/LinkCard.tsx：打开、改网址、去掉链接）。
// 宿主先接（openLink 返回 true：比如本工作区的页面在应用里打开），不接就在新标签页打开。
import { Extension } from '@tiptap/core'
import { Plugin, PluginKey } from '@tiptap/pm/state'

export type OpenLink = (href: string) => boolean

/** 打开一个链接：宿主不接就在新标签页打开（相对地址按当前页面算） */
export function openHref(href: string, host?: OpenLink | null) {
  if (!href) return
  if (host?.(href)) return
  let url: string
  try {
    url = new URL(href, location.href).href
  } catch {
    return
  }
  // 只开网页、邮件这几种，别的（javascript: 之类）不开
  if (!/^(https?|mailto|tel):/i.test(url)) return
  window.open(url, '_blank', 'noopener,noreferrer')
}

export const LinkOpen = Extension.create<{ open: OpenLink }>({
  name: 'oadLinkOpen',
  addOptions() {
    return { open: () => false }
  },
  addProseMirrorPlugins() {
    const ext = this
    return [
      new Plugin({
        key: new PluginKey('oadLinkOpen'),
        props: {
          handleDOMEvents: {
            click: (view, e) => {
              if (e.button !== 0) return false
              const a = (e.target as HTMLElement | null)?.closest?.('a[href]')
              if (!a || !view.dom.contains(a)) return false
              // 只管链接标记画出来的 <a>；网址卡片、页面提及这些节点视图自己处理点击
              if (a.closest('[contenteditable="false"]') && a.closest('[contenteditable="false"]') !== view.dom) return false
              e.preventDefault() // 链接一律由这里打开，别让浏览器自己跟过去
              if (view.editable) {
                // ⌥ / ⇧ 点：只放光标、扩选区；刚拖着选了一段字（松手也会触发 click）：不打开
                if (e.altKey || e.shiftKey) return false
                if (!view.state.selection.empty) return false
              }
              openHref(a.getAttribute('href') ?? '', ext.options.open)
              return true
            },
          },
        },
      }),
    ]
  },
})
