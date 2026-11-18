# CRDT 对比实验：Loro vs Yjs/Yrs

结论：**用 Yjs（浏览器，y-prosemirror / Tiptap Collaboration）+ Yrs（Rust 服务器）。**

## 测了什么

- 文档：Tiptap 3 StarterKit 的 schema，每个块级节点带 `id`。`js/gen.mjs` 生成 1000 / 10000 个顶层块的中英混合文档
  （段落、标题、有序/无序列表、代码块、引用，带加粗、斜体、链接、行内代码）。
- 存法：两边都按各自 ProseMirror 绑定的结构存（Loro：`loro-prosemirror` 的 Map/List/Text；Yjs：`y-prosemirror` 的 XmlFragment），
  这样浏览器直接用现成的绑定，Rust 服务器按同样的结构读写。
- Agent 按段改：Rust 端 `replace_block / insert_after / delete_block`，同类型同编号的节点就地对比着改（属性 diff、子节点按 key 对齐、
  文字按字符 diff 再补 mark），只动变了的字，所以和人同时改同一段时两边的字都留得住。

## 正确性（两边都过）

```
cd js && npm i && node gen.mjs 1000 ../out/base.json && node gen.mjs 10000 ../out/base10k.json && node gen-edits.mjs
cd ../rust && cargo run --release -- build ../out
cd ../js && node user.mjs
cd ../rust && cargo run --release -- agent ../out && cargo run --release -- merge ../out
cd ../js && node final.mjs
```

- Rust 建的文档，两个 JS 绑定都能原样读成 ProseMirror 文档（和原 JSON 一致）。
- 人（JS，经绑定同步）和 Agent（Rust）同时改：同一段里人在中间打字、Agent 改开头和结尾 → 都在；
  Agent 把段落改成标题、在指定段后插两段、改列表里嵌着的段落、删一段；人删一段、加粗 → 都在。
- 两种导入顺序结果一样；JS 和 Rust 合并出来的结果一样；ProseMirror schema 校验通过。

## 性能（Apple Silicon，Node 26，Rust 1.99 release）

浏览器端（`js/bench.mjs`，不含 DOM 渲染）：

| | Loro 1k | Yjs 1k | Loro 10k | Yjs 10k |
| --- | --- | --- | --- | --- |
| 快照大小 | 508 KB | 357 KB | 5049 KB | 3561 KB |
| 打开（导入 + 转 ProseMirror） | 178 ms | 101 ms | 1395 ms | 298 ms |
| **打字，每键 p50 / p95** | **5.3 / 15.5 ms** | **0.14 / 3.0 ms** | **33 / 70 ms** | **3.7 / 8.7 ms** |

服务器端（`cargo run --release -- bench ../out/base.json`）：

| | Loro 1k | Yrs 1k | Loro 10k | Yrs 10k |
| --- | --- | --- | --- | --- |
| 导入 + 读成 JSON | 63 ms | 16 ms | 2629 ms | 344 ms |
| Agent 按段改一次 | 0.64 ms | 0.73 ms | 3.2 ms | 0.17 ms |
| 100 次按段改的增量 | 2 KB | 2 KB | 2 KB | 2 KB |
| 回到旧版本并读出 | 144 ms | 52 ms | 2367 ms | 374 ms |

## 为什么选 Yjs/Yrs

1. **打字延迟**：目标是每键 < 16ms。loro-prosemirror 每次本地改动都要走一遍 JS↔WASM 逐节点比较，1000 段 p95 就到 15ms，
   10000 段 p50 33ms，过不了线；y-prosemirror 在 10000 段时 p95 也只有 8.7ms。
2. **打开和服务器读**都快几倍；快照小 30%。Loro 的格式每个节点要 3–4 个容器，节点一多读起来就慢。
3. **成熟度**：y-prosemirror 是 Tiptap 官方协作扩展用的，大量产品在跑；loro-prosemirror 0.4，远端改动后光标是 setTimeout 再恢复的。
4. Loro 的长处（内置时间旅行、导入快）在这里用不上：Yrs 关掉 gc 后按快照回到旧版本也能做，而且更快。

## 第 1 阶段要注意

- Yrs 往 XmlFragment 中间插节点要从头数位置（O(n)），批量建文档用 `push_back`；按编号找块也是线性的，大文档要建 id → 节点的索引。
- 历史：`gc: false` 文档会一直长。打算定期存快照（SQL 里），旧版本按快照恢复；长期再考虑压缩。
- 两个绑定收到远端改动都是整份替换文档（靠缓存复用没变的节点），评论锚点、装饰要用 Yjs 的相对位置，不能用 ProseMirror 的绝对位置。
- 拆段（回车）时 `id` 不能跟着复制：`keepOnSplit: false` 只管一半，需要一个保证唯一的插件（类似 Tiptap UniqueID）。
- Yrs 0.28 默认 53 位 client id，和 Yjs 13 互通没问题（测过）。
