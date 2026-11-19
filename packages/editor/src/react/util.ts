// 组件里用的小工具：用户颜色、名字缩写、多久以前、页面网址。
import { fmtAgo } from '../i18n'

// ---------- 用户颜色：按编号固定选一个 ----------
const COLORS = ['#e5484d', '#f76b15', '#d6a000', '#30a46c', '#12a594', '#0090ff', '#3e63dd', '#8e4ec6', '#d6409f', '#7c6f64']

export function colorFor(id: string): string {
  let h = 2166136261
  for (let i = 0; i < id.length; i++) h = Math.imul(h ^ id.charCodeAt(i), 16777619)
  return COLORS[(h >>> 0) % COLORS.length]
}

export function initials(name: string): string {
  const s = name.trim()
  if (!s) return '?'
  // 中文名取第一个字，英文取首字母
  const first = [...s][0]
  if (/[㐀-鿿]/.test(first)) return first
  const parts = s.split(/\s+/)
  return (parts[0][0] + (parts[1]?.[0] ?? '')).toUpperCase()
}

export const ago = fmtAgo
