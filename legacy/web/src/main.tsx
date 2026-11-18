import { createRoot } from 'react-dom/client'
import App from './App'
import { SharedPage } from './components/PageView'
import { ImageViewer } from './components/Media'
import { initTheme } from './theme'
import './styles.css'
import 'highlight.js/styles/github-dark.css'

// /s/<token> 是分享出去的页面：不用登录，只显示它本身，不加载应用的其他部分
const shared = location.pathname.match(/^\/s\/([\w-]+)\/?$/)
initTheme()   // 明亮还是夜间：按设置或跟随系统
createRoot(document.getElementById('root')!).render(shared ? <><SharedPage token={shared[1]} /><ImageViewer /></> : <App />)
