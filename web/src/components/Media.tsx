import { useEffect, useRef, useState } from 'react'
import { t } from '../i18n'
import { useBack } from '../swipe'

// 页面里的图片和数据库的文件：上传、看大图。都存在服务端的 media 表（/api/media/<id>，open_agent_docs/media.py）。
// 下载一律用带 download 属性的链接。

export const fmtSize = (n: number) => n < 1024 ? `${n} B` : n < 1 << 20 ? `${(n / 1024).toFixed(1)} KB` : `${(n / (1 << 20)).toFixed(1)} MB`
export const mediaUrl = (id: string) => `/api/media/${id}`

export const VIDEO = /\.(mp4|m4v|webm|mov|ogv)$/i
export const AUDIO = /\.(mp3|m4a|wav|ogg|oga|opus|aac|flac)$/i

// 大图先在浏览器里缩到最长边 MAX 像素再传：显示够用，给模型看也够，电脑没开时也不用去取原图。
// 小图、GIF（可能是动图）原样传；PNG 缩完还是 PNG（截图里的字清楚），其余转 JPEG（浏览器能编码 WebP 的转 WebP）
const MAX = 1600
const SMALL = 1.5 << 20
export async function shrinkImage(f: File): Promise<Blob> {
  if (f.type === 'image/gif') return f
  const url = URL.createObjectURL(f)
  try {
    const img = new Image()
    img.src = url
    await img.decode()
    const w = img.naturalWidth, h = img.naturalHeight
    const native = ['image/png', 'image/jpeg', 'image/webp'].includes(f.type)
    if (native && Math.max(w, h) <= MAX && f.size <= SMALL) return f
    const k = Math.min(1, MAX / Math.max(w, h))
    const c = document.createElement('canvas')
    c.width = Math.round(w * k); c.height = Math.round(h * k)
    const ctx = c.getContext('2d')!
    ctx.imageSmoothingQuality = 'high'
    ctx.drawImage(img, 0, 0, c.width, c.height)
    const blob = (type: string, q?: number) => new Promise<Blob | null>(r => c.toBlob(r, type, q))
    if (f.type === 'image/png') return (await blob('image/png')) || f
    const webp = await blob('image/webp', 0.86)
    return (webp?.type === 'image/webp' ? webp : await blob('image/jpeg', 0.86)) || f
  } finally { URL.revokeObjectURL(url) }
}

export type MediaInfo = { id: string; mime: string; name: string; size: number; w: number; h: number }
export async function uploadMedia(blob: Blob, name: string): Promise<MediaInfo> {
  const r = await fetch(`/api/media?name=${encodeURIComponent(name)}`, { method: 'PUT', body: blob })
  const d = await r.json().catch(() => ({}))
  if (!r.ok) throw new Error(d.detail || t('上传失败（{status}）', { status: r.status }))
  return d
}

// ---------- 看大图、看视频 ----------
// actions：大图下面的按钮（比如“设为头像”）；video：src 是视频，在框里播放
export type ViewerItem = { src: string; name?: string; download?: string; video?: boolean; actions?: { label: string; run: () => void }[] }
export const openImage = (v: ViewerItem) => window.dispatchEvent(new CustomEvent('oad-image', { detail: v }))

export function ImageViewer() {
  const [v, setV] = useState<ViewerItem | null>(null)
  const [err, setErr] = useState('')
  const root = useRef<HTMLDivElement>(null)
  useBack(() => setV(null), root, !!v)
  useEffect(() => {
    const h = (e: Event) => { setErr(''); setV((e as CustomEvent).detail) }
    window.addEventListener('oad-image', h)
    return () => window.removeEventListener('oad-image', h)
  }, [])
  useEffect(() => {
    if (!v) return
    const esc = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.preventDefault(); setV(null) } }
    window.addEventListener('keydown', esc)
    return () => window.removeEventListener('keydown', esc)
  }, [v])
  if (!v) return null
  return (
    <div className="modal-bg viewer" ref={root} onClick={() => setV(null)}>
      {!v.video ? <img src={v.src} alt={v.name || ''} onClick={e => e.stopPropagation()} />
        : err ? <div className="viewer-err" onClick={e => e.stopPropagation()}>{err}</div>
        : <video key={v.src} src={v.src} controls autoPlay playsInline onClick={e => e.stopPropagation()} onError={() => setErr(t('浏览器放不了这个格式，可以下载下来看'))} />}
      <div className="viewer-bar" onClick={e => e.stopPropagation()}>
        {v.name && <span className="viewer-name" title={v.name}>{v.name}</span>}
        {v.actions?.map(a => <button key={a.label} className="btn" onClick={() => { setV(null); a.run() }}>{a.label}</button>)}
        <a className="btn" href={v.download || v.src} download={v.name || true}>{t('下载')}</a>
        <a className="btn" href={v.src} target="_blank" rel="noreferrer" title={t('在新标签页打开')}>↗</a>
        <button className="btn" onClick={() => setV(null)} title={t('关闭')}>×</button>
      </div>
    </div>
  )
}
