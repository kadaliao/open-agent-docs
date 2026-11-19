// 很小的 JSON Schema 检查（只用到 docs/openapi.json 里出现的那些：type、required、properties、items、allOf、oneOf、anyOf、$ref、enum）。
// 返回对不上的地方；extra 里记服务器给了、文档里没写的字段（不算错，但要知道）。
type Js = Record<string, any>

export interface Problem {
  at: string
  msg: string
}

export function checker(spec: Js) {
  const ref = (r: string) => r.split('/').slice(1).reduce((o: Js, k) => o[k], spec)
  const typeOf = (v: unknown) => (v === null ? 'null' : Array.isArray(v) ? 'array' : Number.isInteger(v) ? 'integer' : typeof v)
  const okType = (t: string, v: unknown) => {
    const got = typeOf(v)
    return t === got || (t === 'number' && got === 'integer')
  }

  function check(s: Js, v: unknown, at: string, errs: Problem[], extra: Problem[]) {
    if (!s || typeof s !== 'object') return
    if (s.$ref) return check(ref(s.$ref), v, at, errs, extra)
    if (s.allOf) {
      // allOf：每一个都要满足；多出来的字段看所有分支合起来的
      const props = new Set<string>()
      for (const x of s.allOf) {
        const r = x.$ref ? ref(x.$ref) : x
        Object.keys(r.properties ?? {}).forEach((k) => props.add(k))
        check({ ...r, _noExtra: true }, v, at, errs, extra)
      }
      if (v && typeof v === 'object' && !Array.isArray(v)) for (const k of Object.keys(v)) if (!props.has(k)) extra.push({ at: `${at}.${k}`, msg: '文档里没有' })
      return
    }
    if (s.oneOf || s.anyOf) {
      const alts = s.oneOf ?? s.anyOf
      const ok = alts.some((x: Js) => {
        const e: Problem[] = []
        check(x, v, at, e, [])
        return e.length === 0
      })
      if (!ok) errs.push({ at, msg: `不符合任何一种（${JSON.stringify(v)?.slice(0, 80)}）` })
      return
    }
    if (s.type) {
      const ts: string[] = Array.isArray(s.type) ? s.type : [s.type]
      if (!ts.some((t) => okType(t, v))) {
        errs.push({ at, msg: `应该是 ${ts.join('|')}，实际 ${typeOf(v)}（${JSON.stringify(v)?.slice(0, 60)}）` })
        return
      }
    }
    if (s.enum && !s.enum.includes(v)) errs.push({ at, msg: `不在 ${s.enum.join('/')} 里：${JSON.stringify(v)}` })
    if (Array.isArray(v) && s.items) v.forEach((x, i) => check(s.items, x, `${at}[${i}]`, errs, extra))
    if (v && typeof v === 'object' && !Array.isArray(v) && (s.properties || s.required)) {
      for (const k of s.required ?? []) if (!(k in (v as Js))) errs.push({ at: `${at}.${k}`, msg: '缺了（required）' })
      for (const [k, x] of Object.entries(s.properties ?? {})) if (k in (v as Js)) check(x as Js, (v as Js)[k], `${at}.${k}`, errs, extra)
      if (!s._noExtra && s.properties && s.additionalProperties === undefined)
        for (const k of Object.keys(v as Js)) if (!(k in s.properties)) extra.push({ at: `${at}.${k}`, msg: '文档里没有' })
    }
  }

  /** 找到 method + 实际路径对应的操作，检查这个状态码的响应 */
  function response(method: string, path: string, status: number, body: unknown) {
    const errs: Problem[] = []
    const extra: Problem[] = []
    let op: Js | null = null
    let tmpl = ''
    for (const [p, o] of Object.entries(spec.paths as Js)) {
      const re = new RegExp('^' + p.replace(/\{[^}]+\}/g, '[^/]+') + '$')
      if (re.test(path) && (o as Js)[method.toLowerCase()]) {
        // 有好几个能对上时，挑写死的段多的（/pages/{id}/content 比 /pages/{id} 具体）
        if (!op || p.split('{').length < tmpl.split('{').length) {
          op = (o as Js)[method.toLowerCase()]
          tmpl = p
        }
      }
    }
    if (!op) return { op: `${method} ${path}`, errs: [{ at: '', msg: '文档里没有这个接口' }], extra }
    const r = op.responses?.[String(status)]
    const name = `${method} ${tmpl} → ${status}`
    if (!r) return { op: name, errs: [{ at: '', msg: `文档里没写 ${status} 这个状态码（有 ${Object.keys(op.responses ?? {}).join(', ')}）` }], extra }
    const schema = r.content?.['application/json']?.schema
    if (schema) check(schema, body, '$', errs, extra)
    else if (body !== undefined && status !== 204) extra.push({ at: '$', msg: '文档里没写响应体' })
    return { op: name, errs, extra }
  }

  return { response }
}
