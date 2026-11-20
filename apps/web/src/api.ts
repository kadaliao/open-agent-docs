// 和服务器的 HTTP 接口：用 @open-agent-docs/client（方法、类型都在那里，网页和别的宿主用同一份）。
import { createClient } from '@open-agent-docs/client/core'
import { getLocale } from './i18n'
import { EMBED_AUTH, getToken, watch401 } from './token'
export * from '@open-agent-docs/client/core'

export const API = '/api/v1'

/** 网页用的客户端：同源、cookie 登录、错误信息按界面语言。
 *  跨域嵌入时（token.ts）宿主给了 JWT 就带 Authorization: Bearer，回 401 时向宿主要新的 */
export const client = createClient({
  baseUrl: '',
  locale: () => getLocale(),
  token: () => getToken(),
  ...(EMBED_AUTH ? { fetch: watch401() } : {}),
})

/** 自己发请求（不经过 client）时要带的头：嵌入时的 token */
export function authHeaders(): Record<string, string> {
  const t = getToken()
  return t ? { authorization: 'Bearer ' + t } : {}
}
/** 老名字：每个接口一个方法 */
export const api = client

export const syncUrl = () => `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}${API}/sync`
export const eventsUrl = (ws: string) => `${API}/workspaces/${ws}/events`
