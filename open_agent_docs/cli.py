"""命令行：

  open-agent-docs serve [--host 127.0.0.1] [--port 8790] [--data DIR] [--public-url URL]
  open-agent-docs password            改网页登录密码（会让所有已登录的会话失效）
  open-agent-docs agent add NAME [--scope write] [--webhook URL]   建一个 Agent，打印 token
  open-agent-docs agent list
  open-agent-docs agent revoke ID
"""
import argparse
import getpass
import os
import sys


def _env(args):
    if data := getattr(args, "sub_data", None) or getattr(args, "data", None):
        os.environ["OAD_DATA_DIR"] = data
    if getattr(args, "public_url", None):
        os.environ["OAD_PUBLIC_URL"] = args.public_url


def main(argv=None):
    ap = argparse.ArgumentParser(prog="open-agent-docs", description="Docs that people and AI agents write together.")
    ap.add_argument("--data", help="data directory (default ~/.open-agent-docs, or OAD_DATA_DIR)")
    sub = ap.add_subparsers(dest="cmd")
    s = sub.add_parser("serve", help="run the server")
    s.add_argument("--host", default=os.environ.get("OAD_HOST", "127.0.0.1"))
    s.add_argument("--port", type=int, default=int(os.environ.get("OAD_PORT", "8790")))
    s.add_argument("--public-url", help="public address, e.g. https://docs.example.com (for links sent to agents)")
    s.add_argument("--data", dest="sub_data", help="data directory")
    sub.add_parser("password", help="change the web login password")
    ag = sub.add_parser("agent", help="manage agents").add_subparsers(dest="action")
    a = ag.add_parser("add")
    a.add_argument("name")
    a.add_argument("--scope", default="write", choices=["read", "write", "admin"])
    a.add_argument("--webhook", default="")
    ag.add_parser("list")
    r = ag.add_parser("revoke")
    r.add_argument("id", type=int)
    args = ap.parse_args(argv)
    _env(args)
    if args.cmd in (None, "serve"):
        import uvicorn
        host = getattr(args, "host", "127.0.0.1")
        port = getattr(args, "port", 8790)
        print(f"Open Agent Docs: http://{host}:{port}  (MCP: http://{host}:{port}/mcp)", flush=True)
        uvicorn.run("open_agent_docs.server:app", host=host, port=port, proxy_headers=True, forwarded_allow_ips="*", log_level="warning")
        return
    if args.cmd == "password":
        from . import auth
        pw = getpass.getpass("New password (8+ characters): ")
        if pw != getpass.getpass("Again: "):
            sys.exit("Passwords don't match")
        try:
            auth.set_password(pw)
        except ValueError as e:
            sys.exit(str(e))
        print("Password changed; everyone has been signed out.")
        return
    if args.cmd == "agent":
        from . import agents
        if args.action == "add":
            try:
                rec, token, secret = agents.create(args.name, args.scope, args.webhook)
            except ValueError as e:
                sys.exit(str(e))
            print(f"Agent #{rec['id']} {rec['name']} ({rec['scope']})\ntoken:  {token}\nwebhook secret: {secret}\n"
                  "The token is shown only once.")
        elif args.action == "list":
            for x in agents.all_agents():
                print(f"#{x['id']}\t{x['name']}\t{x['scope']}\t{'revoked' if x['revoked'] else 'active'}\t{x['webhook']}")
        elif args.action == "revoke":
            try:
                agents.revoke(args.id)
            except LookupError as e:
                sys.exit(str(e))
            print("Revoked.")
        else:
            ap.parse_args(["agent", "-h"])
        return


if __name__ == "__main__":
    main()
