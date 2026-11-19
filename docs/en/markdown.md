# Markdown for agents

[简体中文](../zh/markdown.md) · [Contents](README.md)

Pages are stored as structured documents (the same ones the web editor uses); agents read and write them as Markdown. The
conversion round-trips: write back what you read and nothing changes.

## What read_page returns

```
# Q4 plan
page_id: 01JD… · updated 2026-11-18 13:05 UTC · 3 blocks
base: …

⟦b7Kq2⟧
## Goals

⟦Xp0aZ⟧
- Search p95 under 5 ms
- Page history

⟦mN3rT⟧
| Name | Status |
| --- | --- |
| Search | In progress |

Open comments (1 thread, see list_comments):
…
```

- `⟦…⟧` is a **block id**. Only top-level blocks have one (a whole list, a whole table, a code block is one block). Use it to
  target edits; it is not part of the text.
- `base` is the version you read. Pass it back to `edit_page` / `rewrite_page`; the server applies your change on that version and
  merges, so text others typed after your read is not overwritten.
- Long pages come in parts; the end tells you which block to continue from (`from_block`).
- To change one item of a list or table, give the new Markdown of the whole block; the server keeps unchanged items.

## Syntax

| Content | Markdown |
| --- | --- |
| Headings | `#` to `######` |
| Bold, italic, strikethrough, inline code | `**bold**`, `*italic*`, `~~strike~~`, `` `code` `` |
| Underline, highlight | `<u>underline</u>`, `==highlight==` (`<mark>` also works) |
| Links | `[text](https://…)`; bare URLs in text are links too |
| Inline mention (small link chip) | `<https://…>` |
| Link card / video / inline database | A URL **on its own line**: `https://www.youtube.com/watch?v=…`. A URL of a database on this server (`https://docs.example.com/p/<db id>`) shows as an inline table |
| Images | `![alt](src "title")`; a paragraph with only an image is an image block |
| Lists, ordered lists, to-dos | `- item`, `1. item`, `- [ ] todo` / `- [x] done`; indent two spaces to nest |
| Quotes | `> …` |
| Callouts | `> [!NOTE]`, `> [!TIP]`, `> [!IMPORTANT]`, `> [!WARNING]`, `> [!CAUTION]`, optionally followed by a title: `> [!WARNING] Careful` |
| Toggles | `<details><summary>Title</summary>`, blank line, content, blank line, `</details>` |
| Code blocks | ```` ```python ```` … ```` ``` ```` |
| Math | inline `$a^2+b^2$`; block `$$`, newline, formula, newline, `$$` |
| Tables | GFM tables, first row is the header; no merged cells |
| Divider | `---` |
| Line breaks | A single newline inside a paragraph is a line break (no trailing spaces needed) |
| Empty paragraph | `<br>` on its own line |

Unknown HTML is kept as text, not rendered. Display-only settings — image width, captions, code-block wrapping, showing a link
card as a bookmark — have no Markdown form; when an agent rewrites a block that keeps its id, those settings are kept.

Upload files with `upload_file`; it returns Markdown ready for the body (`![…](/api/v1/files/…)` for images, a link otherwise).

## Comments

Comment bodies are Markdown too. Mention people or agents with `@Name`, or `@[Name](user:<id>)` / `@[Name](agent:<id>)`.

## Databases

- Every row is also a page: read and edit a row's body with `read_page` / `edit_page` (`page_id` is the row id); `read_page` on a
  row lists its properties first.
- `db_query` returns a Markdown table with names as values (option names, people's names, titles of related rows) and a `row_id` per row.
- In `db_add` / `db_update`, write properties and values by name: `{"Status": "In progress", "Due": "2026-11-20", "Done": "yes", "Owner": "Ann"}`.
  Unknown select / multi-select options are added. Dates may include a time, `2026-11-15T09:30`; `null` clears a value.
