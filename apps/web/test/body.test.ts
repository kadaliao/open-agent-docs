import { describe, expect, it } from 'vitest'
import { bodyToDraft, draftToBody, mentionedAgents, renderBody } from '../../../packages/editor/src/comments/body'

describe('评论正文', () => {
  it('提到：输入框里显示 @名字，存成 @[名字](agent:<id>)', () => {
    const m = new Map([['写作助手', 'agent:ag1']])
    const body = draftToBody('请 @写作助手 看看，@不认识的 不换', m)
    expect(body).toBe('请 @[写作助手](agent:ag1) 看看，@不认识的 不换')
    expect(mentionedAgents(body)).toEqual(['ag1'])
    const back = bodyToDraft(body)
    expect(back.text).toBe('请 @写作助手 看看，@不认识的 不换')
    expect(back.mentions.get('写作助手')).toBe('agent:ag1')
  })

  it('渲染：转义 HTML，只认少量格式', () => {
    const html = renderBody('<img src=x onerror=alert(1)> **粗** *斜* `a<b` [链接](https://e.com) [坏](javascript:alert(1)) @[张三](user:u1)')
    expect(html).not.toContain('<img')
    expect(html).toContain('&lt;img')
    expect(html).toContain('<strong>粗</strong>')
    expect(html).toContain('<em>斜</em>')
    expect(html).toContain('<code>a&lt;b</code>')
    expect(html).toContain('<a href="https://e.com"')
    expect(html).not.toContain('href="javascript')
    expect(html).toContain('<span class="oad-mention oad-is-user" data-id="u1">@张三</span>')
  })
})
