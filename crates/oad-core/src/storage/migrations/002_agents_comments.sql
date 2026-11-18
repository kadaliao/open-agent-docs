-- Agent：users 里一行（kind = 'agent'），这里放 Agent 才有的字段
CREATE TABLE agents (
    id TEXT PRIMARY KEY REFERENCES users(id),
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    name TEXT NOT NULL,
    about TEXT NOT NULL DEFAULT '',
    scope TEXT NOT NULL,
    token_hash TEXT UNIQUE,
    webhook TEXT,
    -- 签 webhook 要用原文，只能存原文
    webhook_secret TEXT NOT NULL,
    created_by TEXT,
    created_at INTEGER NOT NULL,
    last_used_at INTEGER,
    revoked_at INTEGER
);
CREATE UNIQUE INDEX agents_name ON agents(workspace_id, name);

-- 哪个 Yjs client id 是谁写的
CREATE TABLE doc_clients (
    page_id TEXT NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
    client_id INTEGER NOT NULL,
    principal_kind TEXT NOT NULL,
    principal_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (page_id, client_id)
);

CREATE TABLE comment_threads (
    id TEXT PRIMARY KEY,
    page_id TEXT NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
    block_id TEXT,
    anchor_start TEXT,
    anchor_end TEXT,
    quote TEXT,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    resolved_at INTEGER,
    resolved_by TEXT
);
CREATE INDEX comment_threads_page ON comment_threads(page_id, created_at);

CREATE TABLE comments (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES comment_threads(id) ON DELETE CASCADE,
    author_kind TEXT NOT NULL,
    author_id TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    edited_at INTEGER,
    deleted_at INTEGER
);
CREATE INDEX comments_thread ON comments(thread_id, created_at);

-- Agent 的通知；id 是游标
CREATE TABLE agent_notifications (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL,
    agent_id TEXT NOT NULL REFERENCES agents(id),
    kind TEXT NOT NULL,
    data TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    delivered_at INTEGER,
    acked_at INTEGER,
    attempts INTEGER NOT NULL DEFAULT 0,
    error TEXT,
    -- 下次投 webhook 的时间；NULL = 不用投（没有 webhook、已送到或放弃了）
    next_attempt_at INTEGER
);
CREATE INDEX agent_notifications_agent ON agent_notifications(agent_id, id);
CREATE INDEX agent_notifications_ws ON agent_notifications(workspace_id, id);
CREATE INDEX agent_notifications_due ON agent_notifications(next_attempt_at) WHERE next_attempt_at IS NOT NULL;

-- 人的通知（收件箱）
CREATE TABLE user_notifications (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id TEXT NOT NULL REFERENCES users(id),
    workspace_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    data TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    read_at INTEGER
);
CREATE INDEX user_notifications_user ON user_notifications(user_id, id);
