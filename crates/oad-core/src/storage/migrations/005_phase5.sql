-- 历史版本：种类（auto / agent / restore / manual）、这一版里改过的人、版本的 state vector。state 存 Yrs 快照（PageDoc::version()）
ALTER TABLE doc_versions ADD COLUMN kind TEXT NOT NULL DEFAULT 'auto';
ALTER TABLE doc_versions ADD COLUMN authors TEXT NOT NULL DEFAULT '[]';
ALTER TABLE doc_versions ADD COLUMN state_vector BLOB;

-- 个人偏好：语言、时区（以后用）、头像
ALTER TABLE users ADD COLUMN locale TEXT;
ALTER TABLE users ADD COLUMN timezone TEXT;
ALTER TABLE users ADD COLUMN avatar TEXT;

ALTER TABLE workspaces ADD COLUMN icon TEXT;

-- 公开分享
CREATE TABLE shares (
    id TEXT PRIMARY KEY,
    page_id TEXT NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
    token TEXT NOT NULL UNIQUE,
    include_children INTEGER NOT NULL DEFAULT 0,
    indexable INTEGER NOT NULL DEFAULT 0,
    created_by TEXT,
    created_at INTEGER NOT NULL,
    revoked_at INTEGER
);
CREATE INDEX shares_page ON shares(page_id);

-- 邀请链接
CREATE TABLE invites (
    token TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    created_by TEXT,
    created_at INTEGER NOT NULL,
    expires_at INTEGER,
    revoked_at INTEGER,
    uses INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX invites_ws ON invites(workspace_id);

CREATE INDEX pages_deleted ON pages(workspace_id, deleted_at);

-- 页面用过的上传文件（只加不减：历史版本里可能还用着）。彻底删页面后，没有页面用的文件一起删
CREATE TABLE file_refs (
    page_id TEXT NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
    file_id TEXT NOT NULL,
    PRIMARY KEY (page_id, file_id)
);
CREATE INDEX file_refs_file ON file_refs(file_id);

-- 谁删的（回收站里显示）；一起删的都记上是因为删哪个页面（单独删的记它自己）
ALTER TABLE pages ADD COLUMN deleted_by TEXT;
ALTER TABLE pages ADD COLUMN trash_root TEXT;
