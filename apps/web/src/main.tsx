import { createRoot } from 'react-dom/client'
import '@open-agent-docs/editor/style.css'
import { App } from './App'
import './styles.css'

createRoot(document.getElementById('root')!).render(<App />)

// 离线能打开看过的页面（只在打包后的版本里；开发时不装，免得缓存碍事）
if (import.meta.env.PROD && 'serviceWorker' in navigator) {
  window.addEventListener('load', () => {
    navigator.serviceWorker.register('/sw.js').catch(() => {})
  })
}
