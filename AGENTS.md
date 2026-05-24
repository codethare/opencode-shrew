# ocs — OpenCode Session Viewer

**Commit:** fd17e10 | **Branch:** master | **Rust edition 2024**

## OVERVIEW

CLI tool to read/browse OpenCode session data directly from SQLite (bypassing `opencode export`). Single binary crate, no library, no tests.

## STRUCTURE

```
ocs/
├── src/
│   ├── main.rs           # CLI dispatch (426L) — Cli struct, Commands enum, match arms
│   ├── db.rs             # SQLite data access (1057L) — all queries, read-only by default
│   ├── models.rs         # Data types (310L) — Session, Message, Part, stats, dashboard
│   ├── meta.rs           # JSON sidecar (158L) — tags, notes, autotag rules, atomic write
│   ├── pager.rs          # Terminal pager (372L) — search, goto, scroll, mouse
│   ├── cmd/              # Command handlers (7 files, ~1400L) — see cmd/AGENTS.md
│   └── render/           # Display layer (3 files, ~1425L) — see render/AGENTS.md
├── .github/workflows/
│   ├── ci.yml            # Build + check on push/PR to master
│   └── release.yml       # Cross-platform binary builds on v* tag
├── README.md             # Full CLI reference
└── README.zh.md
```

## WHERE TO LOOK

| Task | File | Notes |
|------|------|-------|
| Add/change CLI arg | `src/main.rs` | Clap derive structs |
| Add/change DB query | `src/db.rs` | All SQLite, parameterized |
| Add/change output format | `src/render/views.rs` | All `render_*` functions |
| Add/change markdown rendering | `src/render/markdown.rs` | pulldown-cmark |
| Add command handler | `src/cmd/` + `src/main.rs` | New file in cmd/, arm in dispatch |
| Tags/notes/autotag | `src/meta.rs` | JSON sidecar |
| TUI pager behavior | `src/pager.rs` | Pager struct, Loader trait |
| Session data model | `src/models.rs` | Structs + serde derives |

## CODE MAP

| Symbol | Type | Location | Role |
|--------|------|----------|------|
| `main()` | fn | `main.rs:361` | CLI entry, open DB, dispatch |
| `Cli` | struct | `main.rs:11` | Clap Parser derive |
| `Commands` | enum | `main.rs:18` | 21 subcommand variants |
| `cmd_*` | 17 fns | `cmd/*.rs` | All command handlers |
| `render_*` | 12+ fns | `render/views.rs` | All display functions |
| `render_markdown()` | fn | `render/markdown.rs` | Markdown→ANSI terminal |
| `Controller` | struct | `render/markdown.rs` | pulldown-cmark event handler |
| `Pager` | struct | `pager.rs:9` | TUI pager state machine |
| `Pager::run()` | fn | `pager.rs:57` | Raw-mode event loop |
| `open_db()` | fn | `db.rs:13` | Read-only connection (PRAGMA query_only) |
| `open_db_rw()` | fn | `db.rs:23` | Read-write connection |
| `OcsMeta` | struct | `meta.rs:10` | Root of JSON sidecar |
| `Session` | struct | `models.rs:4` | Core session model |
| `Message` | struct | `models.rs:25` | Chat message with JSON data |
| `MessageWithParts` | struct | `models.rs:128` | Message + its parts, assembled |

## CONVENTIONS

1. **Edition 2024** — requires Rust 1.85+. No `rust-toolchain.toml` — CI picks up stable.
2. **System SQLite** — `rusqlite` binds to system `libsqlite3` (not bundled). Prerequisite: `apt install libsqlite3-dev` / `brew install sqlite3`.
3. **Read-only by default** — `PRAGMA query_only=ON` for read path. Mutations open separate RW connection.
4. **No tests** — zero `#[test]`, no `tests/` dir. CI runs build + check only.
5. **No lint config** — no clippy.toml, no rustfmt.toml, no deny attributes. Defaults apply.
6. **O_EXCL atomic writes** — meta.rs and safe_write() use O_EXCL temp + rename for TOCTOU safety.
7. **Parameterized SQL only** — never format!() values into SQL strings.
8. **`anyhow` for errors** — Result<T> everywhere, Context for enrichment.

## ANTI-PATTERNS

- **Box::leak for &'static str** — `db.rs:15,25` leak string allocations for Connection::open. Use OnceLock instead.
- **cmd/mod.rs mixing** — shared utilities (safe_write, parse_date, etc.) live in mod.rs alongside module declarations. Should be a separate util file.
- **render/mod.rs mixing** — timestamp helpers, syntax highlighting, strip_ansi live in mod.rs alongside re-exports. Should be separate files.
- **manage.rs + analytics.rs as catch-alls** — 6 commands each in a single file. Splinter if growing.
- **SUBPROCESS_EXIT written but never read** — `cmd/mod.rs:24` AtomicBool set in run.rs but never consumed.
- **Unused params** — `cmd_rename` ignores its `_conn` param, opens its own RW connection. `cmd_report` declares `_project` filter but never applies it.
- **unwrap() on runtime state** — `render/markdown.rs` list_indexes/saved stack pops can panic on malformed markdown.
- **Timestamp math duplicated 6×** — the `ts/1000; rem_euclid; from_timestamp` pattern appears in 6 places. Extract to shared helper.
- **Messages+Parts assembly duplicated 7×** — the query-IDs-then-zip pattern repeats across show, run, manage, analytics. Use `db::get_messages_with_parts_range`.
- **Missing `_project` filter in cmd_report** — declared but not implemented.
- **Missing `_stats_only` filter in cmd_compare** — declared but not implemented.

## COMMANDS

```bash
cargo build --all-targets     # Full build (binary + tests, though no tests exist)
cargo check                   # Verify no warnings
cargo build --release         # Optimized binary at target/release/ocs
```

CI (`ci.yml`): build + check on push/PR to master.  
Release (`release.yml`): tag `v*` triggers cross-build for linux x86_64, macOS x86_64, macOS aarch64. Archives uploaded to GitHub Releases.

No clippy, no fmt, no test in CI.

## NOTES

- DB path: `~/.local/share/opencode/opencode.db` (read-only)
- Meta path: `~/.local/share/opencode/ocs_meta.json` (tags, notes, autotag)
- FTS5 index: `~/.local/share/opencode/ocs_fts.db` (auto-created)
- Diff storage: `~/.local/share/opencode/storage/session_diff/<id>.json`
- Interactive picker requires `peco` or `fzf` in PATH
- Output supports pipe to `$PAGER` / `less -R`
