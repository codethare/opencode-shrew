# ocs — OpenCode Session Viewer

Read and browse [OpenCode](https://github.com/opencode-ai/opencode) session data directly from SQLite, bypassing `opencode export`.

## Features

- **List** sessions with filters (date range, search, project directory, min messages)
- **View** session conversations with markdown rendering, reasoning expansion, and tool call details
- **Search** full-text across all messages with context snippets
- **Stats** per session — token counts, cost, per-agent breakdown (JSON output)
- **Top** sessions ranked by cost, tokens, or message volume
- **Projects** — see session counts grouped by working directory
- **Diff** — review file changes made during a session
- **Rename** sessions without launching the TUI
- **Prune** old sessions with dry-run and confirmation
- **Run** — session-aware `opencode run` wrapper with auto `--dir` from DB, interactive picker with fuzzy search
- **Compare** two sessions side-by-side (markdown table or JSON diff)
- **Annotate** sessions with notes (persisted to JSON sidecar)
- **Report** daily/weekly usage trends with ASCII bar charts
- **Export** sessions as standalone markdown (with YAML frontmatter) or JSON
- **Watch** a session in real-time (poll for new messages)
- **Tag** sessions with custom labels (CRUD via JSON sidecar)
- **Context** — session overview with metadata, tags, stats, and related sessions
- **Autotag** — rule engine for automatic tagging by title, directory, model, cost, or message count
- **FTS5** full-text search index for fast content search
- **Shell completions** generation (bash, zsh, fish, powershell, elvish)
- **JSON output** on list, stats, top, search, projects
- **Pager support** on `show` (pipe to `$PAGER` / `less -R`)

## Install

```bash
git clone <repo>
cd ocs
cargo build --release
# binary at ./target/release/ocs
```

Requires `libsqlite3` (system SQLite). On Debian/Ubuntu: `apt install libsqlite3-dev`. On Arch: `sqlite` is already present.

## Usage

```
Usage: ocs <COMMAND>

Commands:
  list       List sessions
  show       Show a session's messages (omit id for interactive picker)
  stats      Show aggregated stats for a session
  top        Show top sessions by cost, tokens, or message count
  search     Search session content for a query string
  projects   List project directories with session counts
  rename     Rename a session
  prune      Prune old sessions
  diff       Show file diffs for a session
  run        Run opencode as a subprocess (session-aware wrapper)
  compare    Compare two sessions side by side
  annotate   Add or view annotations on a session
  report     Generate a usage report for sessions in a date range
  export     Export a session to a file
  watch      Watch a session in real-time (poll for new messages)
  tag        Manage session tags
  context    Show context for a session (metadata, tags, related sessions, stats)
  completion Generate shell completion scripts
  index      Build FTS5 full-text search index for faster searches
  autotag    Auto-tag sessions using rules
  help       Print this message or the help of the given subcommand(s)
```

### `ocs list`

| Option | Short | Description |
|--------|-------|-------------|
| `--limit N` | `-l` | Max sessions (default 20, 0 = all) |
| `--search TERM` | `-s` | Filter by title or session ID |
| `--since YYYY-MM-DD` | `-f` | Created after this date |
| `--until YYYY-MM-DD` | `-u` | Created before this date |
| `--project DIR` | `-p` | Filter by project directory (basename match) |
| `--compact` | `-c` | Tab-separated one-liner format |
| `--interactive` | `-i` | Pick a session interactively via fuzzy selector |
| `--json` | `-j` | Output as JSON |
| `--annotated` | `-a` | Only show sessions with annotations |
| `--min-msgs N` | | Minimum number of messages (filter out automated/runtime sessions) |

### `ocs show [id]`

Session ID is optional — omit for interactive picker (fuzzy search, then show).

| Option | Short | Description |
|--------|-------|-------------|
| `--raw` | `-r` | Output raw JSON |
| `--no-tool` | `-n` | Hide tool call details |
| `--sanitize` | `-s` | Redact sensitive data in output |
| `--pager` | `-p` | View through pager (uses `$PAGER`, defaults to `less -R`) |

### `ocs stats <id>`

| Option | Short | Description |
|--------|-------|-------------|
| `--json` | `-j` | Output as JSON |

Shows message counts, token usage (input/output/reasoning/cache), total cost, and per-agent breakdown.

### `ocs search <query>`

| Option | Short | Description |
|--------|-------|-------------|
| `--limit N` | `-l` | Max matches (default 20) |
| `--json` | `-j` | Output as JSON |

### `ocs top`

| Option | Short | Description |
|--------|-------|-------------|
| `--limit N` | `-l` | Max sessions (default 10) |
| `--by FIELD` | `-b` | Sort field: `cost` (default), `tokens`, `msgs` |
| `--json` | `-j` | Output as JSON |

### `ocs projects`

| Option | Short | Description |
|--------|-------|-------------|
| `--limit N` | `-l` | Max projects (default 20) |
| `--json` | `-j` | Output as JSON |

### `ocs rename <id> <new-title>`

Renames a session (updates title in-place).

### `ocs prune`

| Option | Short | Description |
|--------|-------|-------------|
| `--older-than DAYS` | `-o` | Delete sessions older than N days (default 30) |
| `--dry-run` | `-n` | Show what would be deleted, don't delete |
| `--force` | `-f` | Skip confirmation prompt |

### `ocs run [message]`

Session-aware wrapper around `opencode run`. Inherits stdin/stdout for interactive use.

| Option | Short | Description |
|--------|-------|-------------|
| `--session ID` | `-s` | Continue this session (auto-sets `--dir` from DB) |
| `--fork` | `-f` | Fork from the specified session |
| `--file PATH` | `-F` | Read message from file (use `-` for stdin) |
| `--interactive` | `-i` | Pick session via fuzzy selector |
| `--project DIR` | `-p` | Filter by project directory (auto-detects cwd in interactive mode) |
| `--since YYYY-MM-DD` | `-S` | Show sessions created after this date |
| `--until YYYY-MM-DD` | `-U` | Show sessions created before this date |

Message sources (in priority order):
1. `[message]` argument — inline text
2. `--file PATH` — read from file, wrapped in ` ``` ` fenced code block
3. `--file -` — read from stdin (pipe-friendly)
4. No args — opens `$EDITOR` / `$VISUAL` (fallback `vim`) for editing

When `--session` or `-i` is provided, ocs looks up the session's working directory from SQLite and passes `--dir <path>` to `opencode run`, so you don't need to `cd` to the right directory first.

### `ocs compare <id1> <id2>`

| Option | Short | Description |
|--------|-------|-------------|
| `--stats` | `-s` | Show only stats comparison (skip message alignment) |
| `--json` | `-j` | Output as JSON |
| `--output PATH` | `-o` | Write to file instead of stdout |

### `ocs annotate <id> [text]`

| Option | Short | Description |
|--------|-------|-------------|
| `--remove` | `-r` | Remove the annotation |

- With `text`: create or update annotation
- Without `text` and no `--remove`: view existing annotation
- With `--remove`: delete annotation

### `ocs report`

| Option | Short | Description |
|--------|-------|-------------|
| `--since YYYY-MM-DD` | `-s` | Start date |
| `--until YYYY-MM-DD` | `-u` | End date (default: today) |
| `--project DIR` | `-p` | Filter by project directory |
| `--format FORMAT` | `-f` | Output format: `markdown` (default) or `json` |
| `--output PATH` | `-o` | Write to file instead of stdout |

Generates a usage report with daily trends (ASCII bar chart), model distribution, and cost summaries.

### `ocs export <id>`

| Option | Short | Description |
|--------|-------|-------------|
| `--format FORMAT` | `-f` | Output format: `markdown` (default) or `json` |
| `--output PATH` | `-o` | Output file path (default: `<title>-<short_id>.<ext>`) |
| `--no-tool` | `-n` | Hide tool call details |
| `--sanitize` | `-s` | Redact sensitive data in output |

### `ocs watch [id]`

| Option | Short | Description |
|--------|-------|-------------|
| `--poll SECS` | `-p` | Poll interval in seconds (default 2) |
| `--no-tool` | `-n` | Hide tool call details |
| `--compact` | `-c` | Compact one-line output |

Polls the database for new messages on a session and renders incrementally.

### `ocs tag [id] [tag]`

| Option | Short | Description |
|--------|-------|-------------|
| `--remove` | `-r` | Remove a tag from a session |
| `--list` | `-l` | List all tags with usage counts |
| `--search TAG` | `-s` | Search sessions by tag |

### `ocs context <id>`

Shows a condensed overview: session metadata, tags, annotation, token usage, agents, and related sessions in the same project.

### `ocs completion <shell>`

Generate shell completion scripts.

```bash
# bash
ocs completion bash > /etc/bash_completion.d/ocs

# zsh
ocs completion zsh > /usr/local/share/zsh/site-functions/_ocs

# fish
ocs completion fish > ~/.config/fish/completions/ocs.fish

# powershell
ocs completion powershell > _ocs.ps1
```

### `ocs index`

| Option | Short | Description |
|--------|-------|-------------|
| `--rebuild` | `-r` | Rebuild the index from scratch |
| `--status` | `-s` | Show index status without building |

Manages the FTS5 full-text search index at `~/.local/share/opencode/ocs_fts.db`. The index is auto-upgraded on first search if missing.

### `ocs autotag <action>`

Actions: `list`, `add`, `remove`, `apply`

| Option | Short | Description |
|--------|-------|-------------|
| `--tag NAME` | `-t` | Tag name (for add/remove) |
| `--title-contains TEXT` | `-T` | Match title containing text (rule condition) |
| `--dir-contains TEXT` | `-d` | Match directory containing text |
| `--model NAME` | `-m` | Match model name |
| `--min-cost N` | | Minimum cost threshold |
| `--max-cost N` | | Maximum cost threshold |
| `--min-msgs N` | | Minimum message count |
| `--max-msgs N` | | Maximum message count |
| `--all` | `-a` | Apply rules to all sessions, not just untagged |

```bash
# Add a rule: auto-tag "high-cost" when cost > $1
ocs autotag add --tag high-cost --min-cost 1.0

# Apply all rules
ocs autotag apply

# List rules
ocs autotag list

# Remove a rule
ocs autotag remove --tag high-cost
```

## Fish Shell Session ID Completion

Add the following to `~/.config/fish/config.fish` to get TAB completion for session IDs:

```fish
function __ocs_list_ids
    ocs list --compact 2>/dev/null | string replace -r '\t.*' ''
end

# Commands taking a session ID as positional argument
complete -c ocs -n "__fish_seen_subcommand_from show stats diff annotate export" -xa "(__ocs_list_ids)"
complete -c ocs -n "__fish_seen_subcommand_from context watch" -xa "(__ocs_list_ids)"
complete -c ocs -n "__fish_seen_subcommand_from rename" -n "__fish_is_nth_token 1" -xa "(__ocs_list_ids)"

# run --session / -s flag
complete -c ocs -n "__fish_seen_subcommand_from run" -s s -l session -xa "(__ocs_list_ids)"
```

After adding, reload with `source ~/.config/fish/config.fish`. Now `ocs show <TAB>`, `ocs run -s <TAB>`, and other session-taking commands will show session IDs with fuzzy filtering.

## Data Source

ocs reads directly from:

```
~/.local/share/opencode/opencode.db
```

The database is treated as read-only (using `PRAGMA query_only=ON`). Write operations (`rename`, `prune`, `index`, `autotag`) open a separate read-write connection.

### Schema

| Table | Contents |
|-------|----------|
| `session` | Session metadata (id, title, directory, timestamps, file change summaries) |
| `message` | Chat messages with JSON `data` column (role, agent, model, tokens, cost) |
| `part` | Message parts with JSON `data` column (text, tool calls, reasoning, step info) |

File diffs are stored externally at `~/.local/share/opencode/storage/session_diff/<id>.json`.

Metadata (tags, notes, autotag rules) is stored at `~/.local/share/opencode/ocs_meta.json`.

## Architecture

```
src/
├── main.rs      # CLI entry point, clap arg parsing, command dispatch (~1400 lines)
├── db.rs        # SQLite queries (list, get, search, stats, top, projects, prune, FTS5, ...)
├── models.rs    # Data structures (Session, Message, Part, + result types)
├── render.rs    # Markdown/compact rendering (list, detail, stats, search, diff, report, ...)
└── meta.rs      # JSON sidecar manager (tags, notes, autotag rules, atomic write)
```

Built with:
- `rusqlite` — system SQLite bindings
- `clap` — command-line argument parsing
- `clap_complete` — shell completion generation
- `serde_json` — JSON deserialization of message/part data
- `chrono` — timestamp formatting
- `anyhow` — error handling
- `dialoguer` — interactive fuzzy selector and confirmation prompts
- `ctrlc` — signal handling for watch mode
