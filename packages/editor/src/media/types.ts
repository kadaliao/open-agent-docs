// 链接解析结果（服务器 GET /api/v1/unfurl 的输出，和 spikes/unfurl/src/model.rs 的 Unfurl 一样）。字段都是尽力而为。
export type UnfurlKind = 'video' | 'post' | 'repo' | 'issue' | 'pull_request' | 'article' | 'image' | 'file' | 'bookmark'
export type Provider = 'youtube' | 'x' | 'github' | 'bilibili' | 'wechat' | 'generic' | string

export interface UAuthor {
  name?: string
  handle?: string
  url?: string
  avatar?: string
}
export interface UImage {
  url: string
  width?: number
  height?: number
  alt?: string
  blurhash?: string
}
export interface UMedia {
  kind: 'image' | 'video' | 'gif' | 'audio'
  url: string
  mime?: string
  width?: number
  height?: number
  poster?: string
  duration_ms?: number
  alt?: string
}

export interface XPost {
  type: 'x'
  id: string
  text: string
  lang?: string
  created_at?: string
  author: UAuthor
  author_verified?: boolean
  urls?: { url: string; expanded_url: string; display_url: string }[]
  mentions?: string[]
  hashtags?: string[]
  media?: UMedia[]
  like_count?: number
  reply_count?: number
  in_reply_to?: string
  possibly_sensitive?: boolean
  quoted?: Omit<XPost, 'type'> & { type?: 'x' }
  card?: { url?: string; title?: string; description?: string; domain?: string; image?: string }
}

export type UEmbed =
  | { type: 'youtube'; video_id: string; start_seconds?: number; is_short: boolean; is_live: boolean; embed_url: string }
  | XPost
  | {
      type: 'github_repo'
      owner: string
      repo: string
      stars?: number
      forks?: number
      language?: string
      license?: string
      topics?: string[]
      pushed_at?: string
      archived?: boolean
    }
  | {
      type: 'github_issue'
      owner: string
      repo: string
      number: number
      is_pull_request: boolean
      state?: string
      state_reason?: string
      labels?: { name: string; color?: string }[]
      comments?: number
      closed_at?: string
      merged_at?: string
      additions?: number
      deletions?: number
      changed_files?: number
      base_ref?: string
      head_ref?: string
    }
  | {
      type: 'bilibili'
      bvid: string
      aid?: number
      cid?: number
      page: number
      start_seconds?: number
      duration_seconds?: number
      view_count?: number
      like_count?: number
      danmaku_count?: number
      embed_url: string
    }
  | { type: 'wechat'; account?: string; account_avatar?: string }
  | { type: 'oembed'; oembed_type: string; provider_name?: string; html?: string; width?: number; height?: number }

export interface Unfurl {
  kind: UnfurlKind
  provider: Provider
  source?: string
  url: string
  canonical_url?: string
  title?: string
  description?: string
  site_name?: string
  favicon?: string
  lang?: string
  author?: UAuthor
  thumbnail?: UImage
  media?: UMedia[]
  published_at?: string
  embed?: UEmbed
  // 上传的文件（/api/v1/files/...）
  name?: string
  mime?: string
  size?: number
  duration_ms?: number
  width?: number
  height?: number
  blurhash?: string
  stale?: boolean
}

/** 上传接口的返回（POST /api/v1/workspaces/{ws}/files） */
export interface Uploaded {
  id?: string
  url: string
  name: string
  mime: string
  size: number
  width?: number
  height?: number
  blurhash?: string
  duration_ms?: number
}
