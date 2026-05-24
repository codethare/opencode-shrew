# render — Display Layer

## OVERVIEW

Converts data models into formatted terminal output. Two concerns: (1) data→string formatting (views.rs), (2) markdown→ANSI parsing (markdown.rs). Shared utilities in mod.rs.

## FILE MAP

| File | Lines | Role |
|------|-------|------|
| `mod.rs` | 115 | Re-exports, timestamp helpers (`ts_to_iso`, `ts_to_compact`), syntax highlighting (`highlight_snippet`), `strip_ansi()`, `visible_width()` |
| `views.rs` | 887 | All `render_*` functions — list, detail, stats, search, diff, compare, export, report, dashboard, context, prune plan |
| `markdown.rs` | 423 | `render_markdown()` + `Controller` struct — pulldown-cmark event handler that produces ANSI terminal output |

## RENDER FUNCTIONS (views.rs)

| Function | Used by | Output |
|----------|---------|--------|
| `render_session_list()` | cmd_list | Formatted table with ID, title, msgs, cost, model, date |
| `render_session_list_compact()` | cmd_list, pick_session_interactive | Tab-separated one-liners |
| `render_session_detail()` | cmd_show | Full conversation with markdown rendering |
| `render_session_raw()` | cmd_show | JSON dump |
| `render_session_stats()` | cmd_stats | Token/cost per-agent breakdown |
| `render_search_results()` | cmd_search | Snippet results with session context |
| `render_diff()` | cmd_diff | File-by-file diff output |
| `render_prune_plan()` | cmd_prune | Dry-run deletion plan |
| `render_report()` | cmd_report | Daily trends + model breakdown with ASCII bars |
| `render_compare()` | cmd_compare | Side-by-side table or JSON diff |
| `render_dashboard()` | cmd_dashboard | Overall stats, model breakdown, project costs |
| `render_context()` | cmd_context | Session overview with metadata + tags |

## MARKDOWN PIPELINE (markdown.rs)

`render_markdown(text) → String`:
1. Preprocess: HTML entity decode (`&amp;` → `&`), heading anchor stripping
2. Parse: `pulldown-cmark::Parser` generates event stream
3. Render: `Controller` handles each event → ANSI terminal (bold headings, colored links, indented blockquotes, fenced code blocks with syntax highlighting)

Uses `syntect` with `base16-ocean.dark` theme for code blocks.

## CONVENTIONS

- Timestamp helpers live in mod.rs but are duplicated across mod.rs + cmd/mod.rs — use mod.rs versions for new code
- `strip_ansi()` is used by pager.rs search — changes must preserve this dependency
- Code highlighting skips blocks >100 lines (performance guard in `highlight_snippet`)
