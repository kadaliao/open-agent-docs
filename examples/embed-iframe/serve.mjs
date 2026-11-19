// 只发一个静态页面（宿主自己的网站）。PORT 默认 5181。
import { readFileSync } from 'node:fs'
import http from 'node:http'

const PORT = Number(process.env.PORT ?? 5181)
const html = readFileSync(new URL('./index.html', import.meta.url))
http.createServer((_, res) => res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' }).end(html)).listen(PORT, () => console.log(`宿主页面：http://localhost:${PORT}/?oad=http://127.0.0.1:8848&page=<页面 id>`))
