// 外壳：没登录显示登录页；登录了左边侧栏（手机上是抽屉），右边按地址显示首页、页面、回收站或设置。
import { useEffect, useRef, useState } from 'react'
import { bootstrap, followUrl, useStore } from './store'
import { useDrawer } from './swipe'
import Sidebar from './components/Sidebar'
import Login from './components/Login'
import Home from './components/Home'
import PageView from './components/PageView'
import Trash from './components/Trash'
import Settings from './components/Settings'
import { ImageViewer } from './components/Media'
import { prefetchDoc } from './components/doc/lazy'

export default function App() {
  const ready = useStore(s => s.ready)
  const authed = useStore(s => s.authed)
  const route = useStore(s => s.route)
  const [side, setSide] = useState(false)   // 手机上的侧栏抽屉
  useDrawer(() => setSide(true), side)      // 左边缘右滑：没有别的层时把它拉出来
  const main = useRef<HTMLElement>(null)

  useEffect(() => {
    bootstrap().catch(() => setTimeout(() => location.reload(), 3000))   // 连不上服务器：过一会儿再试
    window.addEventListener('popstate', followUrl)
    return () => window.removeEventListener('popstate', followUrl)
  }, [])
  useEffect(() => {   // 编辑器（Tiptap 那一大块）先在后台下载好
    if (!authed) return
    const timer = setTimeout(prefetchDoc, 1500)
    return () => clearTimeout(timer)
  }, [authed])
  // 文件拖到没有接收的地方时，别让浏览器直接打开它、离开这个页面
  useEffect(() => {
    const keep = (e: DragEvent) => { if (Array.from(e.dataTransfer?.types || []).includes('Files')) e.preventDefault() }
    window.addEventListener('dragover', keep)
    window.addEventListener('drop', keep)
    return () => { window.removeEventListener('dragover', keep); window.removeEventListener('drop', keep) }
  }, [])

  if (!ready) return <div className="loading">◆</div>
  if (!authed) return <Login />
  return (
    <div className={'app' + (side ? ' side-open' : '')}>
      <button className="mobile-menu icon-btn" onClick={() => setSide(true)}>☰</button>
      <div className="side-mask" onClick={() => setSide(false)} />
      <Sidebar onClose={() => setSide(false)} />
      <main className="chat library" ref={main}>
        {route.name === 'page' ? <PageView key={route.id} id={route.id} frame={main} />
          : route.name === 'trash' ? <Trash />
          : route.name === 'settings' ? <Settings />
          : <Home />}
      </main>
      <ImageViewer />
    </div>
  )
}
