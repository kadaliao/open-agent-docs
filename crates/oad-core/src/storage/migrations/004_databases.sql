-- 页面的种类：page / database（数据库）/ row（库里的一行，parent_id 是库）
ALTER TABLE pages ADD COLUMN kind TEXT NOT NULL DEFAULT 'page';

-- 库：表结构、视图、版本号（库内递增）
CREATE TABLE databases (
    db_id TEXT PRIMARY KEY REFERENCES pages(id) ON DELETE CASCADE,
    schema TEXT NOT NULL,
    views TEXT NOT NULL,
    seq INTEGER NOT NULL DEFAULT 0,
    next_unique INTEGER NOT NULL DEFAULT 1
);

-- 行：属性（JSON，每格最后写入为准）；deleted_at 不为空的进了回收站（rows?since 要告诉客户端）
CREATE TABLE db_rows (
    row_id TEXT PRIMARY KEY REFERENCES pages(id) ON DELETE CASCADE,
    db_id TEXT NOT NULL REFERENCES databases(db_id) ON DELETE CASCADE,
    seq INTEGER NOT NULL,
    position TEXT NOT NULL,
    props TEXT NOT NULL,
    unique_id INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    created_by TEXT,
    updated_at INTEGER NOT NULL,
    updated_by TEXT,
    deleted_at INTEGER
);
CREATE INDEX db_rows_db_seq ON db_rows(db_id, seq);
CREATE INDEX db_rows_db_pos ON db_rows(db_id, position);

-- 每格的改动（历史、撤销、增量同步）。prop = '*' 表示整行（新建 / 删除）
CREATE TABLE db_ops (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    db_id TEXT NOT NULL REFERENCES databases(db_id) ON DELETE CASCADE,
    seq INTEGER NOT NULL,
    row_id TEXT NOT NULL,
    prop TEXT NOT NULL,
    old TEXT,
    new TEXT,
    by TEXT,
    at INTEGER NOT NULL
);
CREATE INDEX db_ops_db_seq ON db_ops(db_id, seq);
CREATE INDEX db_ops_row ON db_ops(row_id, seq);

-- 表结构的改动，能撤销
CREATE TABLE db_schema_ops (
    db_id TEXT NOT NULL REFERENCES databases(db_id) ON DELETE CASCADE,
    seq INTEGER NOT NULL,
    op TEXT NOT NULL,
    before TEXT,
    after TEXT,
    by TEXT,
    at INTEGER NOT NULL,
    PRIMARY KEY (db_id, seq)
);
