// 网页的界面文字都在这里。以后加英文：照这份结构写一份 en，按设置选。
export const zh = {
  appName: 'Open Agent Docs',
  loading: '正在打开…',
  // 登录
  loginTitle: '登录',
  loginSubtitle: '人和 Agent 一起写的文档',
  email: '邮箱',
  password: '密码',
  login: '登录',
  loggingIn: '正在登录…',
  loginFailed: '邮箱或密码不对',
  logout: '退出登录',
  // 侧栏
  pages: '页面',
  newPage: '新建页面',
  newSubpage: '在里面新建子页面',
  rename: '重命名',
  delete: '删除',
  moveToTop: '移到最上面',
  moveOut: '移到上一层',
  moreActions: '更多操作',
  expand: '展开',
  collapse: '收起',
  untitled: '无标题',
  noPages: '还没有页面',
  createFirst: '新建第一页',
  deleteConfirm: '删除“{title}”和它的子页面？',
  deleted: '已删除',
  themeLight: '浅色',
  themeDark: '深色',
  themeSystem: '跟随系统',
  toggleSidebar: '收起或展开侧栏',
  // 页面
  emptyTitle: '选一页开始，或者新建一页',
  emptyHint: '左边是页面树；按 / 插入各种块，选中文字设置格式。',
  pageNotFound: '这一页不存在，或者已经被删除了',
  backHome: '回到首页',
  // 状态
  statusOffline: '离线，改动存在本机',
  statusConnecting: '正在连接',
  statusSyncing: '正在同步',
  statusSynced: '已同步',
  statusLocal: '本机缓存',
  statusDenied: '没有权限',
  readOnly: '只读',
  editingNow: '{n} 人正在看',
  agentWriting: '{name} 正在写',
  you: '你',
  error: '出错了',
  retry: '重试',
}

export type AppStrings = typeof zh
const strings: AppStrings = { ...zh }

export function t(k: keyof AppStrings, vars?: Record<string, string | number>): string {
  let s = strings[k]
  if (vars) for (const [key, v] of Object.entries(vars)) s = s.replace(`{${key}}`, String(v))
  return s
}

export function setAppStrings(s: Partial<AppStrings>) {
  Object.assign(strings, s)
}
// 编辑器自己的文字在 @oad/editor 的 strings.ts，换语言时用 setEditorStrings 一起换
