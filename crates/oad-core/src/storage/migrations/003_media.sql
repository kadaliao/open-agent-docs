-- 链接解析的缓存：url_key 是规范化后的网址
CREATE TABLE unfurls (
    url_key TEXT PRIMARY KEY,
    data TEXT,
    status TEXT NOT NULL,          -- ok / error
    fetched_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,   -- ok：过期时间；error：下次能再试的时间
    error TEXT,
    failures INTEGER NOT NULL DEFAULT 0
);

-- 存下来的内容（按 SHA-256），文件和转存的图片共用
CREATE TABLE blobs (
    hash TEXT PRIMARY KEY,
    size INTEGER NOT NULL,
    mime TEXT NOT NULL,
    width INTEGER,
    height INTEGER,
    blurhash TEXT,
    duration_ms INTEGER,
    created_at INTEGER NOT NULL
);

-- 上传的文件
CREATE TABLE files (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    hash TEXT NOT NULL REFERENCES blobs(hash),
    name TEXT NOT NULL,
    created_by TEXT,
    created_at INTEGER NOT NULL
);
CREATE INDEX files_ws ON files(workspace_id, created_at);

-- 转存的外部图片（卡片里的封面、头像、图标）
CREATE TABLE media (
    hash TEXT PRIMARY KEY REFERENCES blobs(hash),
    source_url TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX media_source ON media(source_url);
