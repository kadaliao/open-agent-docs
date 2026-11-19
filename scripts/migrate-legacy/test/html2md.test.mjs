// 旧版 HTML 页面转 Markdown
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { html2md } from '../html2md.mjs'

test('标题、段落、行内格式、链接、图片', () => {
  const md = html2md('<h2>标题 &amp; 说明</h2><p>一段<b>粗</b>、<em>斜</em>、<s>删</s>、<code>a*b</code>和<a href="https://x.y">链接</a>。<br>换行<img src="/api/media/abc" alt="图"></p>')
  assert.equal(md, '## 标题 & 说明\n\n一段**粗**、*斜*、~~删~~、`a*b`和[链接](https://x.y)。\n换行![图](/api/media/abc)\n')
})

test('嵌套列表、有序列表、引用、代码块、分割线', () => {
  const md = html2md('<ul><li>一<ul><li>一一</li></ul></li><li>二</li></ul><ol><li>甲</li><li>乙</li></ol><blockquote><p>引用</p></blockquote><pre><code>let a = 1 < 2</code></pre><hr>')
  assert.equal(md, '- 一\n  - 一一\n- 二\n\n1. 甲\n2. 乙\n\n> 引用\n\n```\nlet a = 1 < 2\n```\n\n---\n')
})

test('表格', () => {
  const md = html2md('<table><tr><th>名字</th><th>状态</th></tr><tr><td>甲</td><td>完成|好</td></tr></table>')
  assert.equal(md, '| 名字 | 状态 |\n| --- | --- |\n| 甲 | 完成\\|好 |\n')
})

test('不认识的标签留下文字，注释去掉，Markdown 的特殊字符转义', () => {
  assert.equal(html2md('<div><span style="x">普通 *文字*</span><!-- 注释 --></div>'), '普通 \\*文字\\*\n')
  assert.equal(html2md(''), '\n')
})
