// 图片、文件地址的解析钩子：批量签名、缓存到过期前 60 秒、w 自己加、拼服务器地址；图片节点、卡片、大图预览用上它。
import { Editor } from '@tiptap/core'
import { afterEach, describe, expect, it, vi } from 'vitest'
import * as Y from 'yjs'
import { OadCollaboration } from '../src/extensions/collaboration'
import { MediaServicesExt } from '../src/extensions/media'
import { srcset } from '../src/media/dom'
import { clientUrlResolver, createUrlResolver, editQuery, isServerAsset, resolveDeep, type SignResult, type UrlResolver } from '../src/media/resolve'
import { schemaExtensions } from '../src/schema'
import { openLightbox } from '../src/views/image'

const tick = () => new Promise((r) => setTimeout(r, 0))
const flush = async () => {
  for (let i = 0; i < 5; i++) await tick()
}

/** 假的 files/sign：把地址加上 exp、sig，private 开头的签不了 */
function fakeSign(now: () => number, ttl = 3_600_000) {
  const calls: string[][] = []
  const sign = vi.fn(async (urls: string[]): Promise<SignResult> => {
    calls.push(urls)
    const exp = now() + ttl
    return { urls: urls.map((u) => (u.includes('private') ? null : editQuery(u, [], { exp: String(Math.floor(exp / 1000)), sig: 'S' }))), expires_at: exp }
  })
  return { sign, calls }
}

describe('createUrlResolver', () => {
  it('不是服务器文件的原样给；同一轮的攒成一批签，签好的缓存起来同步给', async () => {
    let t = 1_000_000
    const { sign, calls } = fakeSign(() => t)
    const r = createUrlResolver({ baseUrl: 'https://docs.example.com/', sign, now: () => t })
    expect(r('https://i.com/a.png')).toBe('https://i.com/a.png')
    expect(r('data:image/png;base64,xx')).toBe('data:image/png;base64,xx')
    const a = r('/api/v1/files/abcdefghijkl/a.png')
    const b = r('/api/v1/files/abcdefghijkl/a.png?w=320')
    const c = r('/api/v1/media/ff00')
    expect(a).toBeInstanceOf(Promise)
    expect(await a).toBe('https://docs.example.com/api/v1/files/abcdefghijkl/a.png?exp=4600&sig=S')
    // w 不进签名：同一个文件只签一次，w 加在后面
    expect(await b).toBe('https://docs.example.com/api/v1/files/abcdefghijkl/a.png?exp=4600&sig=S&w=320')
    expect(await c).toBe('https://docs.example.com/api/v1/media/ff00?exp=4600&sig=S')
    expect(calls).toEqual([['/api/v1/files/abcdefghijkl/a.png', '/api/v1/media/ff00']])
    // 缓存：同步给，换个 w 也行
    expect(r('/api/v1/files/abcdefghijkl/a.png?w=640')).toBe('https://docs.example.com/api/v1/files/abcdefghijkl/a.png?exp=4600&sig=S&w=640')
    expect(sign).toHaveBeenCalledTimes(1)
    // 离过期不到 60 秒：重签
    t += 3_600_000 - 59_000
    const again = r('/api/v1/files/abcdefghijkl/a.png')
    expect(again).toBeInstanceOf(Promise)
    await again
    expect(sign).toHaveBeenCalledTimes(2)
    // fresh：不用缓存
    expect(r('/api/v1/media/ff00', { fresh: true })).toBeInstanceOf(Promise)
    await flush()
    expect(sign).toHaveBeenCalledTimes(3)
  })

  it('签不了的照原样（拼上服务器地址），一会儿再问；接口出错不记', async () => {
    let t = 0
    const { sign } = fakeSign(() => t)
    const r = createUrlResolver({ baseUrl: 'https://d.example.com', sign, now: () => t })
    expect(await r('/api/v1/files/private00000/x.png')).toBe('https://d.example.com/api/v1/files/private00000/x.png')
    expect(r('/api/v1/files/private00000/x.png')).toBe('https://d.example.com/api/v1/files/private00000/x.png')
    t += 61_000
    expect(r('/api/v1/files/private00000/x.png')).toBeInstanceOf(Promise)

    const bad = createUrlResolver({ baseUrl: '', sign: async () => Promise.reject(new Error('断网')) })
    expect(await bad('/api/v1/files/abcdefghijkl/a.png')).toBe('/api/v1/files/abcdefghijkl/a.png')
    expect(bad('/api/v1/files/abcdefghijkl/a.png')).toBeInstanceOf(Promise)
    await flush()
  })

  it('已经带签名的、不签的（只拼地址）', async () => {
    const sign = vi.fn()
    const r = createUrlResolver({ baseUrl: 'https://d.example.com', sign })
    expect(r('/api/v1/files/abcdefghijkl/a.png?exp=1&sig=x')).toBe('https://d.example.com/api/v1/files/abcdefghijkl/a.png?exp=1&sig=x')
    expect(sign).not.toHaveBeenCalled()
    const plain = createUrlResolver({ baseUrl: 'https://d.example.com' })
    expect(plain('/api/v1/files/abcdefghijkl/a.png')).toBe('https://d.example.com/api/v1/files/abcdefghijkl/a.png')
  })

  it('一批最多 500 个', async () => {
    const { sign, calls } = fakeSign(() => 0)
    const r = createUrlResolver({ sign })
    const all = Array.from({ length: 501 }, (_, i) => r(`/api/v1/files/${String(i).padStart(12, '0')}`))
    await Promise.all(all)
    expect(calls.map((c) => c.length)).toEqual([500, 1])
  })
})

describe('clientUrlResolver', () => {
  const client = (baseUrl: string) => ({ baseUrl, signFileUrls: fakeSign(() => 0).sign })
  it('同域 cookie 登录不用换；token 登录签名；跨域只拼地址', async () => {
    expect(clientUrlResolver(client(''))).toBeNull()
    expect(clientUrlResolver(client(location.origin))).toBeNull()
    const tok = clientUrlResolver(client(''), { token: true })!
    expect(await tok('/api/v1/files/abcdefghijkl/a.png')).toContain('sig=S')
    const cross = clientUrlResolver(client('https://other.example.com'))!
    expect(cross('/api/v1/files/abcdefghijkl/a.png')).toBe('https://other.example.com/api/v1/files/abcdefghijkl/a.png')
  })
})

describe('别的小工具', () => {
  it('srcset 换 w、留着签名', () => {
    expect(srcset('https://d.example.com/api/v1/files/a/b.png?exp=1&sig=S&w=99')).toBe(
      [320, 640, 960, 1280, 1920].map((w) => `https://d.example.com/api/v1/files/a/b.png?exp=1&sig=S&w=${w} ${w}w`).join(', '),
    )
    expect(srcset('/api/v1/media/ff')?.startsWith('/api/v1/media/ff?w=320 320w')).toBe(true)
    expect(srcset('https://i.com/a.png')).toBeUndefined()
    expect(isServerAsset('https://x.com/api/v1/files/a')).toBe(true)
    expect(isServerAsset('/api/v1/pages/a')).toBe(false)
  })

  it('resolveDeep 换掉对象里所有服务器地址', async () => {
    const r: UrlResolver = (u) => (u.includes('later') ? Promise.resolve(u + '?s') : u + '?s')
    const d = { url: '/api/v1/files/a/x.mp4', thumbnail: { url: '/api/v1/media/h' }, title: 't', media: [{ url: 'https://i.com/a.png' }] }
    expect(resolveDeep(d, r)).toEqual({ url: '/api/v1/files/a/x.mp4?s', thumbnail: { url: '/api/v1/media/h?s' }, title: 't', media: [{ url: 'https://i.com/a.png' }] })
    const p = resolveDeep({ a: '/api/v1/files/later' }, r)
    expect(p).toBeInstanceOf(Promise)
    expect(await p).toEqual({ a: '/api/v1/files/later?s' })
    expect(resolveDeep(d, null)).toBe(d)
  })

  it('大图预览只去掉 w，签名留着', () => {
    openLightbox('https://d.example.com/api/v1/files/a/b.png?exp=1&sig=S&w=640', 'a')
    const img = document.querySelector('.oad-lightbox img') as HTMLImageElement
    expect(img.getAttribute('src')).toBe('https://d.example.com/api/v1/files/a/b.png?exp=1&sig=S')
    document.querySelectorAll('.oad-lightbox').forEach((m) => m.remove())
  })
})

describe('图片节点用上钩子', () => {
  const editors: Editor[] = []
  afterEach(() => editors.splice(0).forEach((e) => e.destroy()))

  it('显示签好的地址（srcset 也是），文档里的地址不变；加载失败重签一次', async () => {
    let n = 0
    const resolveUrl: UrlResolver = vi.fn((u: string) => Promise.resolve(`https://d.example.com${u}?exp=9&sig=S${++n}`))
    const ydoc = new Y.Doc()
    const el = document.createElement('div')
    document.body.append(el)
    const editor = new Editor({
      element: el,
      extensions: [...schemaExtensions(), OadCollaboration.configure({ fragment: ydoc.getXmlFragment('default') }), MediaServicesExt.configure({ get: () => ({ resolveUrl, unfurl: async () => null }) })],
    })
    editors.push(editor)
    editor.commands.setContent({ type: 'doc', content: [{ type: 'image', attrs: { src: '/api/v1/files/abcdefghijkl/a.png' } }] })
    await flush()
    const img = el.querySelector('figure.oad-image img') as HTMLImageElement
    expect(img.getAttribute('src')).toBe('https://d.example.com/api/v1/files/abcdefghijkl/a.png?exp=9&sig=S1')
    expect(img.getAttribute('srcset')).toContain('https://d.example.com/api/v1/files/abcdefghijkl/a.png?exp=9&sig=S1&w=320 320w')
    expect(editor.getJSON().content?.[0].attrs?.src).toBe('/api/v1/files/abcdefghijkl/a.png')
    img.dispatchEvent(new Event('error'))
    await flush()
    expect(resolveUrl).toHaveBeenLastCalledWith('/api/v1/files/abcdefghijkl/a.png', { fresh: true })
    expect(img.getAttribute('src')).toContain('sig=S2')
    // 第二次失败：显示坏图
    img.dispatchEvent(new Event('error'))
    expect(el.querySelector('figure.oad-image')!.classList.contains('oad-is-broken')).toBe(true)
    el.remove()
  })
})
