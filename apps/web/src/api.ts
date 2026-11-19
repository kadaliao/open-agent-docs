// 和服务器的 HTTP 接口：用 @oad/client（方法、类型都在那里，网页和别的宿主用同一份）。
import { createClient } from '@oad/client/core'
import { getLocale } from './i18n'
export * from '@oad/client/core'

export const API = '/api/v1'

/** 网页用的客户端：同源、cookie 登录、错误信息按界面语言 */
export const client = createClient({ baseUrl: '', locale: () => getLocale() })
/** 老名字：每个接口一个方法 */
export const api = client

export const syncUrl = () => `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}${API}/sync`
export const eventsUrl = (ws: string) => `${API}/workspaces/${ws}/events`
