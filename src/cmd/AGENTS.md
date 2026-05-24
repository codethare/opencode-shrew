# cmd — Command Handlers

## OVERVIEW

All CLI command implementations. Each file is one logical group; `mod.rs` re-exports all `cmd_*` functions and provides shared utilities.

## FILE MAP

| File | Exports | Role |
|------|---------|------|
| `mod.rs` | `cmd_*` re-exports, `safe_write()`, `parse_date()`, `format_timestamp()`, `validate_session_id()`, `pick_session_interactive()`, `SUBPROCESS_EXIT` | Module hub + shared helpers |
| `list.rs` | `cmd_list()` | `ocs list` — filtered session listing |
| `show.rs` | `cmd_show()`, `cmd_search()`, `cmd_diff()` | View sessions, search, diffs |
| `run.rs` | `cmd_run()`, `cmd_watch()` | `opencode run` wrapper + polling watch |
| `manage.rs` | `cmd_rename()`, `cmd_prune()`, `cmd_tag()`, `cmd_annotate()`, `cmd_undo()`, `cmd_export()` | Session mutations (rename, prune, tag, annotate, undo, export) |
| `analytics.rs` | `cmd_stats()`, `cmd_top()`, `cmd_projects()`, `cmd_report()`, `cmd_compare()`, `cmd_dashboard()` | Aggregation + comparison |
| `completion.rs` | `cmd_completion()` | Shell completion gen via clap_complete |

## DISPATCH PATTERN

All `cmd_*` functions follow: `fn cmd_*(conn: &Connection, ...) -> Result<()>`.  
Main dispatch in `src/main.rs` match arms passes parsed args straight through.

## ADDING A NEW COMMAND

1. Create `src/cmd/new_feature.rs` with `pub fn cmd_new_feature(...) -> Result<()>`
2. Add `mod new_feature;` + `pub use new_feature::cmd_new_feature;` in `mod.rs`
3. Add variant to `Commands` enum + match arm in `main.rs`

## SHARED UTILITIES (mod.rs)

- `validate_session_id()` — rejects `..`, `/`, `\0` (path traversal guard)
- `parse_date()` — `"YYYY-MM-DD"` → epoch ms
- `safe_write()` — O_EXCL temp + rename atomic write
- `pick_session_interactive()` — pipes to peco/fzf

## CAVEATS

- `manage.rs` + `analytics.rs` are catch-alls — split out if they exceed 500 lines each
- `cmd_rename()` ignores its `_conn` param (opens own RW connection)
- `cmd_report()` has declared `_project` filter that is NOT implemented
- `cmd_compare()` has declared `_stats_only` flag that is NOT implemented
- `SUBPROCESS_EXIT` AtomicBool in mod.rs is set but never read
