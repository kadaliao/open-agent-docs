// 端到端测试里的“库”：默认打真服务器（POST /workspaces/{ws}/databases、/rows……）；OAD_DB_MOCK=1 时用 db-mock.ts 在浏览器里拦请求。
import { request as pwRequest, test, type APIRequestContext, type BrowserContext, type Page } from '@playwright/test'
import { DbMock } from './db-mock'

type Json = Record<string, any>
export const MOCK = process.env.OAD_DB_MOCK === '1'

export interface Backend {
  id: string
  /** 浏览器上下文装上需要的拦截（真服务器时什么都不做） */
  install(ctx: BrowserContext): Promise<void>
  /** 按位置排好的行 */
  rows(): Promise<Json[]>
  schema(): Promise<Json>
  /** 下一次写行（PATCH rows）返回 500 */
  failNextWrite(page: Page): Promise<void>
  /** 另一个客户端改了一行 */
  remoteSet(rowId: string, set: Json): Promise<void>
  /** 真服务器会推 db.changed；假的要手动同步 */
  live: boolean
}

export async function makeDb(request: APIRequestContext, ws: string, title: string, schema: Json, views: Json[], rows: Json[], mockId = 'db' + Math.random().toString(36).slice(2, 8)): Promise<Backend> {
  if (MOCK) {
    const mock = new DbMock(ws)
    mock.add(mockId, title, schema, views, rows)
    return {
      id: mockId,
      live: false,
      install: (ctx) => mock.install(ctx),
      rows: async () => [...mock.dbs.get(mockId)!.rows.values()].sort((a, b) => (a.position < b.position ? -1 : 1)),
      schema: async () => mock.dbs.get(mockId)!.schema,
      failNextWrite: async () => {
        mock.failNext = true
      },
      remoteSet: async (rowId, set) => {
        mock.remoteSet(mockId, rowId, set)
      },
    }
  }
  // 测试的 request 每个测试一份、测完就关；这里自己开一个，整个文件用
  void request
  request = await pwRequest.newContext({ baseURL: test.info().project.use.baseURL })
  const r = await request.post(`/api/v1/workspaces/${ws}/databases`, { data: { title, schema, views } })
  if (!r.ok()) throw new Error('建库失败：' + (await r.text()))
  const id = (await r.json()).page.id as string
  // 一次加 1000 行
  for (let i = 0; i < rows.length; i += 1000) {
    const add = await request.post(`/api/v1/databases/${id}/rows`, { data: { rows: rows.slice(i, i + 1000).map((x) => ({ props: x.props, ...(x.body ? { body: x.body } : {}) })) } })
    if (!add.ok()) throw new Error('加行失败：' + (await add.text()))
  }
  return {
    id,
    live: true,
    install: async () => {},
    rows: async () => ((await (await request.get(`/api/v1/databases/${id}/rows`)).json()).rows as Json[]).sort((a, b) => (a.position < b.position ? -1 : 1)),
    schema: async () => (await (await request.get(`/api/v1/databases/${id}`)).json()).schema,
    failNextWrite: async (page) => {
      let done = false
      await page.route(`**/api/v1/databases/${id}/rows`, (route) => {
        if (!done && route.request().method() === 'PATCH') {
          done = true
          return route.fulfill({ status: 500, json: { error: { code: 'internal', message: '模拟的服务器错误' } } })
        }
        return route.fallback()
      })
    },
    remoteSet: async (rowId, set) => {
      const p = await request.patch(`/api/v1/databases/${id}/rows`, { data: { ops: [{ row_id: rowId, set }] } })
      if (!p.ok()) throw new Error(await p.text())
    },
  }
}
