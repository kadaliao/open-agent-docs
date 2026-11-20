# Migrating from the old version

[简体中文](../zh/migration.md) · [Contents](README.md)

The old version (Python, `uv run open-agent-docs`, data in `~/.open-agent-docs/docs.db`) stores data differently, so you can't
just swap the program. `scripts/migrate-legacy` imports the old data through the new server's API. The old version doesn't need
to be running: the tool only reads its database file and never changes it.

## Steps

1. Back up the old data directory (one file, `docs.db`).
2. Start the new version ([Quick start](quickstart.md)) and create the admin.
3. Prepare the tool in the repo (Node 22.5 or newer):
   ```sh
   pnpm install && pnpm --filter @open-agent-docs/client build
   ```
4. See what would be imported (writes nothing):
   ```sh
   node scripts/migrate-legacy/migrate.mjs --from ~/.open-agent-docs --to http://127.0.0.1:8848 \
     --token "$OAD_TOKEN" --dry-run
   ```
5. Import:
   ```sh
   node scripts/migrate-legacy/migrate.mjs --from ~/.open-agent-docs --to http://127.0.0.1:8848 \
     --token "$OAD_TOKEN" --tokens-out ./agent-tokens.txt
   ```
6. Read the report `oad-migration-report.md`, put the new tokens from `agent-tokens.txt` into each agent's configuration, then
   delete that file.

Signing in: `--token` takes an `admin` agent token (create one in the new web app) or a session token. Not needed when the new
server runs with `OAD_AUTH=none`; with `header` auth use `--header x-oad-user-id:me --header x-oad-secret:…`. Imported content is
written as that identity; the old version's "me" in person properties becomes the person who created the token (or the first owner).

## Options

| Option | Meaning |
| --- | --- |
| `--from` | The old data directory, or the `docs.db` file |
| `--to` | The new server, default `http://127.0.0.1:8848` (or `OAD_URL`) |
| `--token` | Token for the new server (or `OAD_TOKEN`) |
| `--header k:v` | Extra request header; repeatable |
| `--workspace` | Workspace to import into; the first one by default |
| `--map` | Mapping file (old ids → new ids), default `./oad-migration-map.json` |
| `--report` | Report, default `./oad-migration-report.md` |
| `--tokens-out` | Where to write the recreated agents' new tokens (mode 600). Agents are skipped without it |
| `--dry-run` | Show what would be imported; write nothing |
| `--no-agents`, `--no-comments`, `--no-shares`, `--no-trash` | Skip these |

## What's imported

| Old | New |
| --- | --- |
| Pages, subpages, order | The same tree. Markdown bodies as they are; HTML pages converted to Markdown (common tags only) |
| Images and files | Re-uploaded; addresses in bodies and file properties rewritten |
| Databases | Schema (options and colors, status groups, number formats, unique-id prefixes), rows and row bodies, views (sorts, filters, hidden columns, board grouping) |
| Relations, rollups, formulas, buttons | Relations (two-way too) mapped to the new row ids; rollups, formulas and buttons recreated. A formula the new version can't parse becomes a text column, listed in the report |
| Comments | Imported as threads with replies and resolved state. All attached to the whole page, with the quoted text at the start; comments by agents start with the original author |
| Agents | Recreated (name, description, scope, webhook). The **new tokens** go only to the `--tokens-out` file, never to the screen. Revoked agents are skipped |
| Public shares | New links, with an old → new list in the report (old links stop working) |
| Trash | Plain pages are imported and then deleted, so they sit in the new trash |

Not imported: page version history (the new history starts at the import) and pinning. Created/edited time and created-by reflect
the import; unique ids are renumbered in import order. The report lists all of this (the tool's output and report are in Chinese).

## Running it again

The mapping file records everything imported. Running again only adds what's missing; nothing is duplicated. If a run stops
halfway (network, server restart), just run it again. To start over, wipe the new server's data and the mapping file.

## Tests

```sh
OAD_BIN=target/release/open-agent-docs node scripts/migrate-legacy/test-e2e.mjs
```

It builds realistic old data with the old version's own code (`cd legacy && uv run …`), starts a new server, runs `--dry-run`
(checking nothing was written), imports and checks every item, then runs again and checks nothing was added.
