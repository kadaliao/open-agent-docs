-- 链接解析的缓存：url_key 是规范化后的网址
CREATE TABLE unfurls (
    url_key TEXT COLLATE "C" PRIMARY KEY,
    data TEXT COLLATE "C",
    status TEXT COLLATE "C" NOT NULL,          -- ok / error
    fetched_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,   -- ok：过期时间；error：下次能再试的时间
    error TEXT COLLATE "C",
    failures BIGINT NOT NULL DEFAULT 0
);

-- 存下来的内容（按 SHA-256），文件和转存的图片共用
CREATE TABLE blobs (
    hash TEXT COLLATE "C" PRIMARY KEY,
    size BIGINT NOT NULL,
    mime TEXT COLLATE "C" NOT NULL,
    width BIGINT,
    height BIGINT,
    blurhash TEXT COLLATE "C",
    duration_ms BIGINT,
    created_at BIGINT NOT NULL
);

-- 上传的文件
CREATE TABLE files (
    id TEXT COLLATE "C" PRIMARY KEY,
    workspace_id TEXT COLLATE "C" NOT NULL REFERENCES workspaces(id),
    hash TEXT COLLATE "C" NOT NULL REFERENCES blobs(hash),
    name TEXT COLLATE "C" NOT NULL,
    created_by TEXT COLLATE "C",
    created_at BIGINT NOT NULL
);
CREATE INDEX files_ws ON files(workspace_id, created_at);

-- 转存的外部图片（卡片里的封面、头像、图标）
CREATE TABLE media (
    hash TEXT COLLATE "C" PRIMARY KEY REFERENCES blobs(hash),
    source_url TEXT COLLATE "C" NOT NULL,
    created_at BIGINT NOT NULL
);
CREATE INDEX media_source ON media(source_url);
