# ocv — OpenCode Session Viewer

Read and browse [OpenCode](https://github.com/opencode-ai/opencode) session data directly from SQLite, bypassing `opencode export`.

## Features

- **List** sessions with filters (date range, search, project directory)
- **View** session conversations with markdown rendering, reasoning expansion, and tool call details
- **Search** full-text across all messages with context snippets
- **Stats** per session — token counts, cost, per-agent breakdown
- **Top** sessions ranked by cost, tokens, or message volume
- **Projects** — see session counts grouped by working directory
- **Diff** — review file changes made during a session
- **Rename** sessions without launching the TUI
- **Prune** old sessions with dry-run and confirmation
- **Run** — session-aware `opencode run` wrapper with auto `--dir` from DB

## Install

```bash
git clone <repo>
cd ocv
cargo build --release
# binary at ./target/release/ocv
```

Requires `libsqlite3` (system SQLite). On Debian/Ubuntu: `apt install libsqlite3-dev`. On Arch: `sqlite` is already present.

## Usage

```
ocv <COMMAND>

Commands:
  list      List sessions
  show      Show a session's messages
  stats     Show aggregated stats for a session
  top       Show top sessions by cost, tokens, or message count
  search    Search session content for a query string
  projects  List project directories with session counts
  rename    Rename a session
  prune     Prune old sessions
  diff      Show file diffs for a session
  run       Run opencode as a subprocess (session-aware)
  help      Print this message or the help of the given subcommand(s)
```

### `ocv list`

| Option | Description |
|--------|-------------|
| `-l`, `--limit N` | Max sessions (default 20, 0 = all) |
| `-s`, `--search TERM` | Filter by title or session ID |
| `--since YYYY-MM-DD` | Created after this date |
| `--until YYYY-MM-DD` | Created before this date |
| `--project DIR` | Filter by project directory (partial match) |
| `--compact` | Tab-separated one-liner format |
| `-i`, `--interactive` | Pipe through peco/fzf for selection |

### `ocv show <id>`

| Option | Description |
|--------|-------------|
| `--raw` | Output raw JSON |
| `--no-tool` | Hide tool call details |

### `ocv stats <id>`

Shows message counts, token usage (input/output/reasoning/cache), total cost, and per-agent breakdown.

### `ocv top`

| Option | Description |
|--------|-------------|
| `-l`, `--limit N` | Max sessions (default 10) |
| `-b`, `--by FIELD` | Sort field: `cost` (default), `tokens`, `msgs` |

### `ocv search <query>`

| Option | Description |
|--------|-------------|
| `-l`, `--limit N` | Max matches (default 20) |

### `ocv projects`

| Option | Description |
|--------|-------------|
| `-l`, `--limit N` | Max projects (default 20) |

### `ocv rename <id> <new-title>`

Renames a session (updates title in-place).

### `ocv prune`

| Option | Description |
|--------|-------------|
| `-d`, `--older-than DAYS` | Delete sessions older than N days (default 30) |
| `--dry-run` | Show what would be deleted, don't delete |
| `--force` | Skip confirmation prompt |

### `ocv run <message>`

Session-aware wrapper around `opencode run`. Inherits stdin/stdout for interactive use.

| Option | Description |
|--------|-------------|
| `-s`, `--session ID` | Continue this session (auto-sets `--dir` from DB) |
| `-f`, `--fork` | Fork from the specified session |
| `-i`, `--interactive` | Pick session interactively via peco/fzf |

When `--session` or `-i` is provided, ocv looks up the session's working directory from SQLite and passes `--dir <path>` to `opencode run`, so you don't need to `cd` to the right directory first.

### `ocv diff <id>`

Shows file changes (unified diff) recorded for a session, if available.

## Examples

```bash
# List recent sessions
ocv list

# Filter by project
ocv list --project omocode

# Compact format, pipe to peco
ocv list --compact -i

# View a session
ocv show ses_abc123

# View without tool details
ocv show ses_abc123 --no-tool

# Search across all sessions
ocv search "error handling"

# See which sessions cost the most
ocv top --by cost --limit 5

# Stats for a session
ocv stats ses_abc123

# What projects have sessions?
ocv projects

# Rename a session
ocv rename ses_abc123 "My new title"

# See what old sessions exist
ocv prune --older-than 60 --dry-run

# Delete them
ocv prune --older-than 60 --force

# View file diffs
ocv diff ses_abc123

# Run a new prompt in an existing session (auto-sets --dir from DB)
ocv run -s ses_abc123 "continue implementing this feature"

# Fork from a session to try a different approach
ocv run -s ses_abc123 --fork "try a different approach"

# Interactive: pick a session via peco/fzf, then run
ocv run -i "continue from selected session"
```

## Data Source

ocv reads directly from:

```
~/.local/share/opencode/opencode.db
```

The database is treated as read-only (using `PRAGMA query_only=ON`). Write operations (`rename`, `prune`) open a separate read-write connection.

### Schema

| Table | Contents |
|-------|----------|
| `session` | Session metadata (id, title, directory, timestamps, file change summaries) |
| `message` | Chat messages with JSON `data` column (role, agent, model, tokens, cost) |
| `part` | Message parts with JSON `data` column (text, tool calls, reasoning, step info) |

File diffs are stored externally at `~/.local/share/opencode/storage/session_diff/<id>.json`.

## Architecture

```
src/
├── main.rs      # CLI entry point, clap arg parsing, command dispatch
├── db.rs        # SQLite queries (list, get, search, stats, top, projects, prune)
├── models.rs    # Data structures (Session, Message, Part, + result types)
└── render.rs    # Markdown/compact rendering (list, detail, stats, search, diff, etc.)
```

Built with:
- `rusqlite` — system SQLite bindings
- `clap` — command-line argument parsing
- `serde_json` — JSON deserialization of message/part data
- `chrono` — timestamp formatting
- `anyhow` — error handling
