import { useId } from 'react'
// 界面里用到的几个小图标
export function SidebarIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 18 18" fill="none" aria-hidden>
      <rect x="2.25" y="3" width="13.5" height="12" rx="2.25" stroke="currentColor" strokeWidth="1.4" />
      <path d="M7 3v12" stroke="currentColor" strokeWidth="1.4" />
    </svg>
  )
}

// 产品 Logo：深色圆角底上三条文本块，中间那块（Agent 改的那一块）是渐变色
export function Logo({ size = 22 }: { size?: number }) {
  const id = useId().replace(/:/g, '')
  return (
    <svg width={size} height={size} viewBox="0 0 512 512" aria-hidden className="oad-logo">
      <defs>
        <linearGradient id={`${id}t`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#2c2c30" />
          <stop offset="1" stopColor="#0b0b0d" />
        </linearGradient>
        <linearGradient id={`${id}g`} x1="0" y1="0" x2="1" y2="0">
          <stop offset="0" stopColor="#8b7bff" />
          <stop offset=".55" stopColor="#e86bd0" />
          <stop offset="1" stopColor="#ffab6b" />
        </linearGradient>
      </defs>
      <rect width="512" height="512" rx="114" fill={`url(#${id}t)`} />
      <rect x="2" y="2" width="508" height="508" rx="112" fill="none" stroke="#fff" strokeOpacity=".12" strokeWidth="4" />
      <rect x="116" y="125" width="280" height="66" rx="33" fill="#f5f5f7" />
      <rect x="116" y="223" width="220" height="66" rx="33" fill={`url(#${id}g)`} />
      <rect x="116" y="321" width="150" height="66" rx="33" fill="#f5f5f7" fillOpacity=".5" />
    </svg>
  )
}
