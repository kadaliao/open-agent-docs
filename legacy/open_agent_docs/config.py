"""配置：都从环境变量来，命令行（cli.py）会先把参数写进环境变量再启动。

- OAD_DATA_DIR：数据放哪（SQLite 数据库，图片也在库里）。默认 ~/.open-agent-docs。
- OAD_PUBLIC_URL：网页地址的开头（比如 https://docs.example.com）。给 Agent 的页面链接、webhook 里的链接用；
  不设就按请求的 Host 算（webhook 里没有请求，就只给相对地址）。
- OAD_LANG：界面和工具文字的默认语言（zh / en）。设置页里改过的以设置为准。
- OAD_PASSWORD：网页登录密码。不设时第一次启动随机生成一个、打印在终端里（只打这一次），之后用 `open-agent-docs password` 改。
- OAD_SECURE_COOKIE：1 = 登录 cookie 只走 https（放在 https 反代后面时打开）。
"""
import os
from pathlib import Path

DATA_DIR = Path(os.environ.get("OAD_DATA_DIR") or Path.home() / ".open-agent-docs").expanduser()
DB_PATH = DATA_DIR / "docs.db"
PUBLIC_URL = (os.environ.get("OAD_PUBLIC_URL") or "").rstrip("/")
LANG = os.environ.get("OAD_LANG") or ""
PASSWORD = os.environ.get("OAD_PASSWORD") or ""
SECURE_COOKIE = os.environ.get("OAD_SECURE_COOKIE", "") in ("1", "true", "yes")
VERSION = "0.1.0"
