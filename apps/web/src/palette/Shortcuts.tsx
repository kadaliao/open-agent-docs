// ⌘/ 快捷键帮助。和 Notion 的习惯一致；Mac 上显示 ⌘ ⌥ ⇧，别的系统显示 Ctrl Alt Shift。
import { useEffect } from 'react'
import { t, type AppStrings } from '../strings'

const mac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent)
const keyName = (k: string) =>
  mac ? k : k.replace(/⌘/g, 'Ctrl+').replace(/⌥/g, 'Alt+').replace(/⇧/g, 'Shift+').replace(/\+(?=[^+]*$)/, '+')

type Row = [keyof AppStrings, string]
const GROUPS: { title: keyof AppStrings; rows: Row[] }[] = [
  {
    title: 'scGeneral',
    rows: [
      ['scSearch', '⌘K'],
      ['scHelp', '⌘/'],
      ['scTheme', '⌘⇧L'],
      ['scSidebar', '⌘\\'],
      ['scBack', '⌘[ ⌘]'],
      ['scUndo', '⌘Z ⌘⇧Z'],
    ],
  },
  {
    title: 'scBlocks',
    rows: [
      ['scSlash', '/'],
      ['scTurnText', '⌘⌥0'],
      ['scTurnH', '⌘⌥1 2 3'],
      ['scTurnTodo', '⌘⌥4'],
      ['scTurnBullet', '⌘⌥5'],
      ['scTurnNumbered', '⌘⌥6'],
      ['scTurnToggle', '⌘⌥7'],
      ['scTurnCode', '⌘⌥8'],
      ['scMove', '⌘⇧↑ ⌘⇧↓'],
      ['scDuplicate', '⌘D'],
      ['scIndent', 'Tab ⇧Tab'],
    ],
  },
  {
    title: 'scText',
    rows: [
      ['scBold', '⌘B'],
      ['scItalic', '⌘I'],
      ['scUnderline', '⌘U'],
      ['scStrike', '⌘⇧S'],
      ['scCode', '⌘E'],
      ['scHighlight', '⌘⇧H'],
      ['scLink', '⌘K'],
      ['scComment', '⌘⇧M'],
    ],
  },
  {
    title: 'scMarkdown',
    rows: [
      ['scMdHeading', '# ## ###'],
      ['scMdBullet', '- *'],
      ['scMdNumbered', '1.'],
      ['scMdTodo', '[]'],
      ['scMdQuote', '>'],
      ['scMdCode', '```'],
      ['scMdDivider', '---'],
      ['scMdInline', '**b** *i* `c` ~~s~~'],
    ],
  },
]

function Keys({ k, md }: { k: string; md?: boolean }) {
  return (
    <span className="sc-keys">
      {k.split(' ').map((x, i) => (
        <kbd key={i} className={md ? 'is-md' : undefined}>
          {md ? x : keyName(x)}
        </kbd>
      ))}
    </span>
  )
}

export function ShortcutHelp({ onClose }: { onClose: () => void }) {
  useEffect(() => {
    const k = (e: KeyboardEvent) => e.key === 'Escape' && onClose()
    window.addEventListener('keydown', k)
    return () => window.removeEventListener('keydown', k)
  }, [onClose])
  return (
    <div className="modal-mask" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal sc-modal" role="dialog" aria-label={t('shortcutsTitle')}>
        <div className="modal-head">
          <h2>{t('shortcutsTitle')}</h2>
          <button className="icon-btn" onClick={onClose} aria-label={t('cancel')}>
            ×
          </button>
        </div>
        <div className="sc-grid">
          {GROUPS.map((g) => (
            <section key={g.title} className="sc-group">
              <h3>{t(g.title)}</h3>
              {g.rows.map(([label, k]) => (
                <div key={label} className="sc-row">
                  <span>{t(label)}</span>
                  <Keys k={k} md={g.title === 'scMarkdown'} />
                </div>
              ))}
            </section>
          ))}
        </div>
      </div>
    </div>
  )
}
