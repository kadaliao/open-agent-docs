CREATE TABLE workspaces (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE users (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT UNIQUE COLLATE NOCASE,
    password_hash TEXT,
    external_id TEXT UNIQUE,
    kind TEXT NOT NULL DEFAULT 'user',
    created_at INTEGER NOT NULL
);

CREATE TABLE members (
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    PRIMARY KEY (workspace_id, user_id)
);
CREATE INDEX members_user ON members(user_id);

CREATE TABLE sessions (
    token_hash TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE INDEX sessions_user ON sessions(user_id);

CREATE TABLE pages (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    parent_id TEXT REFERENCES pages(id),
    position TEXT NOT NULL,
    title TEXT NOT NULL DEFAULT '',
    icon TEXT,
    created_by TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER
);
CREATE INDEX pages_tree ON pages(workspace_id, parent_id, position);
CREATE INDEX pages_parent ON pages(parent_id);

-- 正文：快照 + 之后的增量（Yrs v1）
CREATE TABLE doc_snapshots (
    page_id TEXT PRIMARY KEY REFERENCES pages(id) ON DELETE CASCADE,
    state BLOB NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE doc_updates (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    page_id TEXT NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
    data BLOB NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX doc_updates_page ON doc_updates(page_id, seq);

-- 历史版本（第 5 阶段用）
CREATE TABLE doc_versions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    page_id TEXT NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
    state BLOB NOT NULL,
    label TEXT,
    created_by TEXT,
    created_at INTEGER NOT NULL
);
CREATE INDEX doc_versions_page ON doc_versions(page_id, created_at);
