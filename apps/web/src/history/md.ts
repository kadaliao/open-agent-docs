// 一行 Markdown 里的行内格式 → HTML（先全部转义，再认 **粗** *斜* `码` ~~删~~ [链接](网址)）。只给历史对比这种只读的地方用。
export function inlineMd(s: string): string {
  let h = s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!)
  h = h.replace(/`([^`]+)`/g, '<code>$1</code>')
  h = h.replace(/\*\*([^*]+)\*\*/g, '<b>$1</b>')
  h = h.replace(/(^|[^*])\*([^*\s][^*]*)\*/g, '$1<i>$2</i>')
  h = h.replace(/~~([^~]+)~~/g, '<s>$1</s>')
  h = h.replace(/\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g, '<a href="$2" target="_blank" rel="noreferrer">$1</a>')
  return h.replace(/\n/g, '<br>')
}
