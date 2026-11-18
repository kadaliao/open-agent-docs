// 页面标题，绑定 Y.Text "title"。本地改动只把变了的那一截写进去（前后相同的部分不动），别人同时改标题也不会互相覆盖。
import { forwardRef, useCallback, useEffect, useImperativeHandle, useLayoutEffect, useRef } from 'react'
import * as Y from 'yjs'
import { t } from './strings'

export interface OadTitleProps {
  ydoc: Y.Doc
  field?: string
  readOnly?: boolean
  placeholder?: string
  className?: string
  // 回车 / 在末尾按下方向键：跳到正文
  onEnter?: () => void
  onChange?: (title: string) => void
}

export interface OadTitleHandle {
  focus: (atEnd?: boolean) => void
}

/** 把 a 改成 b 的最小一段替换：返回 [起点, 删几个, 插什么]。按 UTF-16 算，和 Y.Text 一致。 */
export function minimalDiff(a: string, b: string): [number, number, string] {
  let start = 0
  const max = Math.min(a.length, b.length)
  while (start < max && a.charCodeAt(start) === b.charCodeAt(start)) start++
  // 不把代理对拆开
  if (start > 0 && start < a.length && isLowSurrogate(a.charCodeAt(start))) start--
  let endA = a.length
  let endB = b.length
  while (endA > start && endB > start && a.charCodeAt(endA - 1) === b.charCodeAt(endB - 1)) {
    endA--
    endB--
  }
  if (endA < a.length && isLowSurrogate(a.charCodeAt(endA))) {
    endA++
    endB++
  }
  return [start, endA - start, b.slice(start, endB)]
}

const isLowSurrogate = (c: number) => c >= 0xdc00 && c <= 0xdfff

export function applyTitle(ytext: Y.Text, next: string, origin?: unknown) {
  const cur = ytext.toString()
  if (cur === next) return
  const [at, del, ins] = minimalDiff(cur, next)
  ytext.doc!.transact(() => {
    if (del) ytext.delete(at, del)
    if (ins) ytext.insert(at, ins)
  }, origin)
}

export const OadTitle = forwardRef<OadTitleHandle, OadTitleProps>(function OadTitle(
  { ydoc, field = 'title', readOnly, placeholder, className, onEnter, onChange },
  ref,
) {
  const el = useRef<HTMLTextAreaElement>(null)
  const composing = useRef(false)
  const ytext = ydoc.getText(field)
  const origin = useRef({})

  const fit = useCallback(() => {
    const ta = el.current
    if (!ta) return
    ta.style.height = '0px'
    ta.style.height = ta.scrollHeight + 'px'
  }, [])

  useImperativeHandle(ref, () => ({
    focus(atEnd = true) {
      const ta = el.current
      if (!ta) return
      ta.focus()
      if (atEnd) ta.setSelectionRange(ta.value.length, ta.value.length)
    },
  }))

  // 远端改动：更新文字，光标跟着改动挪
  useLayoutEffect(() => {
    const ta = el.current!
    ta.value = ytext.toString()
    fit()
    onChange?.(ta.value)
    const obs = (e: Y.YTextEvent) => {
      if (e.transaction.origin === origin.current) return
      const focused = document.activeElement === ta
      let s = ta.selectionStart
      let en = ta.selectionEnd
      let pos = 0
      for (const op of e.delta) {
        if (op.retain) pos += op.retain
        else if (typeof op.insert === 'string') {
          const n = op.insert.length
          if (s >= pos) s += n
          if (en >= pos) en += n
          pos += n
        } else if (op.delete) {
          const n = op.delete
          if (s > pos) s = Math.max(pos, s - n)
          if (en > pos) en = Math.max(pos, en - n)
        }
      }
      ta.value = ytext.toString()
      if (focused) ta.setSelectionRange(s, en)
      fit()
      onChange?.(ta.value)
    }
    ytext.observe(obs)
    return () => ytext.unobserve(obs)
  }, [ytext]) // eslint-disable-line react-hooks/exhaustive-deps

  // 宽度变了（包括从隐藏变成可见：隐藏时量出来是 0）、字体加载完，都重新量高度
  useEffect(() => {
    const ta = el.current!
    let width = -1
    const ro = new ResizeObserver(([e]) => {
      const w = e.contentRect.width
      if (w !== width) {
        width = w
        fit()
      }
    })
    ro.observe(ta)
    document.fonts?.ready.then(fit)
    return () => ro.disconnect()
  }, [fit])

  // Chromium 偶尔在插入文字后把光标放回开头（e2e 里用 CDP 输入时约四分之一会碰到），按插入前的位置纠正回来
  useEffect(() => {
    const ta = el.current!
    let expect = -1
    const before = (e: InputEvent) => {
      expect = e.inputType === 'insertText' && e.data && ta.selectionStart === ta.selectionEnd ? ta.selectionStart + e.data.length : -1
    }
    const after = () => {
      if (expect >= 0 && ta.selectionStart !== expect && ta.selectionStart === ta.selectionEnd) ta.setSelectionRange(expect, expect)
      expect = -1
    }
    ta.addEventListener('beforeinput', before)
    ta.addEventListener('input', after, true)
    return () => {
      ta.removeEventListener('beforeinput', before)
      ta.removeEventListener('input', after, true)
    }
  }, [])

  const push = () => {
    const ta = el.current!
    // 标题是一行：粘贴进来的换行换成空格
    const v = ta.value.replace(/\r?\n/g, ' ')
    if (v !== ta.value) {
      const p = ta.selectionStart
      ta.value = v
      ta.setSelectionRange(p, p)
    }
    applyTitle(ytext, v, origin.current)
    fit()
    onChange?.(v)
  }

  return (
    <textarea
      ref={el}
      className={'oad-title' + (className ? ' ' + className : '')}
      rows={1}
      readOnly={readOnly}
      spellCheck={false}
      placeholder={placeholder ?? t('titlePlaceholder')}
      aria-label={placeholder ?? t('titlePlaceholder')}
      onCompositionStart={() => (composing.current = true)}
      onCompositionEnd={() => {
        composing.current = false
        push()
      }}
      onInput={() => {
        if (!composing.current) push()
      }}
      onKeyDown={(e) => {
        if (e.nativeEvent.isComposing) return
        const ta = e.currentTarget
        if (e.key === 'Enter' || (e.key === 'ArrowDown' && ta.selectionStart === ta.value.length)) {
          e.preventDefault()
          onEnter?.()
        }
      }}
    />
  )
})
