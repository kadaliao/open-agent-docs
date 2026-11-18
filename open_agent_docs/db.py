"""SQLite：所有表都在一个库里（页面、版本、评论、数据库、图片、Agent、通知）。各模块在自己文件顶上建自己的表。"""
import sqlite3
from .config import DB_PATH

DB_PATH.parent.mkdir(parents=True, exist_ok=True)
conn = sqlite3.connect(DB_PATH, check_same_thread=False)
conn.execute("pragma journal_mode=wal")
conn.execute("pragma busy_timeout=5000")
conn.execute("create table if not exists settings(key text primary key, value text)")
conn.commit()
