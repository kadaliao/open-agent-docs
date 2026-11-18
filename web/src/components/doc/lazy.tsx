// 主包里唯一引用编辑器的地方：编辑器（Tiptap 那一大块）只在打开 markdown 页面时才加载，聊天不用多下载。
// 加载出来之前、或者加载失败（服务器换了新版、旧的分块已经删了）时，显示原来的只读页面，并提示刷新。
import { Component, Suspense, lazy, type ReactNode } from 'react'
import Markdown from '../Markdown'
import { markNewBuild } from '../../store'

const Editor = lazy(() => import('./DocEditor'))
let fetched = false
export const prefetchDoc = () => {
  if (fetched) return
  fetched = true
  import('./DocEditor').catch(() => { fetched = false })
}

const CHUNK = /dynamically imported module|Importing a module script failed|Failed to fetch|error loading/i

class Boundary extends Component<{ fallback: ReactNode; children: ReactNode }, { failed: boolean }> {
  state = { failed: false }
  static getDerivedStateFromError() { return { failed: true } }
  componentDidCatch(e: Error) { if (CHUNK.test(String(e?.message || e))) markNewBuild() }
  render() { return this.state.failed ? this.props.fallback : this.props.children }
}

export function DocPage({ id, content }: { id: number; content: string }) {
  const fallback = <div className="page-md"><Markdown text={content} /></div>
  return (
    <Boundary key={id} fallback={fallback}>
      <Suspense fallback={fallback}><Editor id={id} fallback={content} /></Suspense>
    </Boundary>
  )
}
