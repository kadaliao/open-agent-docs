// 编辑器里的界面文字都在这里。换语言：setEditorStrings({ ... }) 传一份覆盖。
export const zh = {
  placeholder: '输入文字，或按 / 插入内容',
  emptyPlaceholder: '开始写点什么…',
  untitled: '无标题',
  titlePlaceholder: '无标题',
  // 斜杠菜单
  slashEmpty: '没有匹配的块',
  'block.paragraph': '正文',
  'block.h1': '一级标题',
  'block.h2': '二级标题',
  'block.h3': '三级标题',
  'block.bulletList': '无序列表',
  'block.orderedList': '有序列表',
  'block.taskList': '待办清单',
  'block.blockquote': '引用',
  'block.callout': '提示框',
  'block.details': '折叠块',
  'block.codeBlock': '代码块',
  'block.mathBlock': '公式',
  'block.table': '表格',
  'block.horizontalRule': '分割线',
  'block.image': '图片',
  'block.embed': '链接卡片',
  'blockHint.paragraph': '普通文字',
  'blockHint.h1': '大号标题',
  'blockHint.h2': '中号标题',
  'blockHint.h3': '小号标题',
  'blockHint.bulletList': '圆点列表',
  'blockHint.orderedList': '带序号的列表',
  'blockHint.taskList': '可勾选的待办',
  'blockHint.blockquote': '引用一段话',
  'blockHint.callout': '醒目的提示',
  'blockHint.details': '可展开收起',
  'blockHint.codeBlock': '等宽代码',
  'blockHint.mathBlock': 'LaTeX 公式',
  'blockHint.table': '三行三列',
  'blockHint.horizontalRule': '分隔内容',
  'blockHint.image': '用网址插入图片',
  'blockHint.embed': '网址显示成卡片',
  // 提示框
  'callout.note': '说明',
  'callout.tip': '提示',
  'callout.important': '重要',
  'callout.warning': '注意',
  'callout.caution': '警告',
  calloutTitle: '标题（可不填）',
  // 公式
  mathEmpty: '点这里输入 LaTeX 公式',
  mathPlaceholder: '例如 E = mc^2',
  done: '完成',
  // 代码
  codeLanguage: '语言',
  plainText: '纯文本',
  // 链接、图片
  linkPlaceholder: '粘贴或输入链接',
  imagePlaceholder: '图片网址',
  embedPlaceholder: '网址',
  confirm: '确定',
  removeLink: '去掉链接',
  openLink: '打开链接',
  // 格式
  bold: '粗体',
  italic: '斜体',
  underline: '下划线',
  strike: '删除线',
  code: '行内代码',
  highlight: '高亮',
  link: '链接',
  mathInline: '行内公式',
  comment: '评论',
  // 表格
  addRowAbove: '上方插入行',
  addRowBelow: '下方插入行',
  addColLeft: '左侧插入列',
  addColRight: '右侧插入列',
  deleteRow: '删除行',
  deleteCol: '删除列',
  deleteTable: '删除表格',
  // 折叠块
  expand: '展开',
  collapse: '收起',
  detailsSummary: '折叠块标题',
  // 链接卡片
  embedHint: '链接卡片',
  // 卡片
  playVideo: '播放视频',
  openOriginal: '打开原网页',
  clickToLoad: '点一下加载',
  live: '直播',
  views: '播放',
  danmaku: '弹幕',
  xUnavailable: '帖子暂时取不到，点开在 X 上看',
  replyingTo: '回复',
  viewOnX: '在 X 上查看',
  archived: '已归档',
  updatedAt: '更新于',
  merged: '已合并',
  closed: '已关闭',
  draft: '草稿',
  open: '开放',
  download: '下载',
  preview: '预览',
  collapsePreview: '收起预览',
  // 粘贴链接后的菜单
  pasteAsEmbed: '嵌入',
  pasteAsBookmark: '书签',
  pasteAsMention: '提及',
  pasteAsLink: '链接',
  pasteHint: '回车 = 嵌入',
  // 图片
  captionPlaceholder: '写图注…',
  uploading: '正在上传',
  uploadFailed: '上传失败',
  zoomIn: '查看大图',
  // 代码
  copyCode: '复制',
  copied: '已复制',
  wrapOn: '自动换行',
  showSource: '源码',
  showDiagram: '图',
  diagramError: '图画不出来：',
  // 块菜单
  dragHandle: '拖动移动，点一下打开菜单',
  turnInto: '转换成',
  copyBlockLink: '复制块链接',
  duplicate: '复制一份',
  deleteBlock: '删除',
  viewAs: '显示成',
  // 协作
  you: '你',
}

export type EditorStrings = typeof zh

export const strings: EditorStrings = { ...zh }

export function setEditorStrings(s: Partial<EditorStrings>) {
  Object.assign(strings, s)
}

export const t = (k: keyof EditorStrings) => strings[k]
