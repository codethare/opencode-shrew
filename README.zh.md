# ocs — OpenCode 会话查看器

直接从 SQLite 读取和浏览 [OpenCode](https://github.com/opencode-ai/opencode) 会话数据，绕过 `opencode export`。

## 功能

- **列表** 会话，支持筛选（日期范围、搜索、项目目录、最少消息数）
- **查看** 会话对话，支持 markdown 渲染、推理过程展开、工具调用详情
- **搜索** 所有消息全文搜索（FTS5 索引），附带上下文片段
- **统计** 每会话 Token 统计、费用、按代理细分（支持 JSON 输出）
- **排行** 按费用、Token、消息量排名会话
- **项目** 按工作目录分组查看会话数量
- **差异** 查看会话期间的文件变更（unified diff）
- **重命名** 无需启动 TUI 即可重命名会话
- **清理** 支持 dry-run 和确认提示的旧会话清理
- **运行** session 感知的 `opencode run` 封装，自动从数据库读取 `--dir`，支持交互式模糊搜索选择器
- **对比** 并排对比两个会话（markdown 表格或 JSON diff）
- **注释** 为会话添加持久化的笔记（JSON sidecar）
- **报告** 每日/每周使用趋势，含 ASCII 柱状图
- **导出** 将会话导出为独立 markdown（含 YAML frontmatter）或 JSON
- **监视** 实时监视会话新消息（轮询模式）
- **标签** 为会话添加自定义标签（JSON sidecar CRUD）
- **上下文** 会话概览：元数据、标签、统计、相关会话
- **自动标签** 规则引擎：按标题、目录、模型、费用、消息数自动打标
- **全文搜索索引** FTS5 索引加速搜索
- **Shell 补全** 生成 bash/zsh/fish/powershell/elvish 补全脚本
- **JSON 输出** list/stats/top/search/projects 均支持
- **分页器** show 命令支持 `$PAGER` / `less -R`

## 安装

```bash
git clone <repo>
cd ocs
cargo build --release
# 二进制文件位于 ./target/release/ocs
```

需要系统 SQLite (`libsqlite3`)。Debian/Ubuntu：`apt install libsqlite3-dev`。Arch：`sqlite` 已预装。

## 使用方法

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

| 选项 | 短名 | 说明 |
|------|------|------|
| `--limit N` | `-l` | 最大显示数（默认 20，0 为全部） |
| `--search TERM` | `-s` | 按标题或 ID 筛选 |
| `--since YYYY-MM-DD` | `-f` | 此日期之后创建的会话 |
| `--until YYYY-MM-DD` | `-u` | 此日期之前创建的会话 |
| `--project DIR` | `-p` | 按项目目录筛选（basename 精确匹配） |
| `--compact` | `-c` | Tab 分隔的紧凑格式 |
| `--interactive` | `-i` | 通过模糊搜索器交互式选择会话 |
| `--json` | `-j` | 输出为 JSON |
| `--annotated` | `-a` | 仅显示有注释的会话 |
| `--min-msgs N` | | 最少消息数过滤（排除自动化/运行时产生的小会话） |

### `ocs show [id]`

会话 ID 可选 — 省略则进入交互式选择器（模糊搜索，选中后显示）。

| 选项 | 短名 | 说明 |
|------|------|------|
| `--raw` | `-r` | 输出原始 JSON |
| `--no-tool` | `-n` | 隐藏工具调用详情 |
| `--sanitize` | `-s` | 脱敏敏感数据 |
| `--pager` | `-p` | 通过分页器查看（使用 `$PAGER`，默认 `less -R`） |

### `ocs stats <id>`

| 选项 | 短名 | 说明 |
|------|------|------|
| `--json` | `-j` | 输出为 JSON |

显示消息数、Token 用量（输入/输出/推理/缓存）、总费用和按代理细分。

### `ocs search <query>`

| 选项 | 短名 | 说明 |
|------|------|------|
| `--limit N` | `-l` | 最大匹配数（默认 20） |
| `--json` | `-j` | 输出为 JSON |

### `ocs top`

| 选项 | 短名 | 说明 |
|------|------|------|
| `--limit N` | `-l` | 最大显示数（默认 10） |
| `--by FIELD` | `-b` | 排序字段：`cost`（默认）、`tokens`、`msgs` |
| `--json` | `-j` | 输出为 JSON |

### `ocs projects`

| 选项 | 短名 | 说明 |
|------|------|------|
| `--limit N` | `-l` | 最大项目数（默认 20） |
| `--json` | `-j` | 输出为 JSON |

### `ocs rename <id> <new-title>`

原地重命名会话（更新标题）。

### `ocs prune`

| 选项 | 短名 | 说明 |
|------|------|------|
| `--older-than DAYS` | `-o` | 删除 N 天前的会话（默认 30） |
| `--dry-run` | `-n` | 预览要删除的内容，不实际删除 |
| `--force` | `-f` | 跳过确认提示 |

### `ocs diff <id>`

显示会话期间记录的文件变更（unified diff 格式）。

### `ocs run [message]`

`opencode run` 的 session 感知封装。继承 stdin/stdout 用于交互式使用。

| 选项 | 短名 | 说明 |
|------|------|------|
| `--session ID` | `-s` | 继续此会话（自动从数据库设置 `--dir`） |
| `--fork` | `-f` | 从指定会话派生 |
| `--file PATH` | `-F` | 从文件读取消息（使用 `-` 从 stdin 读取） |
| `--interactive` | `-i` | 通过模糊搜索器选择会话 |
| `--project DIR` | `-p` | 按项目目录筛选（交互模式下自动检测 cwd） |
| `--since YYYY-MM-DD` | `-S` | 显示此日期之后创建的会话 |
| `--until YYYY-MM-DD` | `-U` | 显示此日期之前创建的会话 |

消息来源（优先级顺序）：
1. `[message]` 参数 — 直接输入文本
2. `--file PATH` — 从文件读取，包裹在 ` ``` ` 代码块中
3. `--file -` — 从 stdin 读取（适合管道）
4. 无参数 — 打开 `$EDITOR` / `$VISUAL`（默认 `vim`）编辑

当指定 `--session` 或 `-i` 时，ocs 从 SQLite 查找会话的工作目录，并自动传递 `--dir <path>` 给 `opencode run`。

### `ocs compare <id1> <id2>`

| 选项 | 短名 | 说明 |
|------|------|------|
| `--stats` | `-s` | 仅显示统计对比（跳过消息对齐） |
| `--json` | `-j` | 输出为 JSON |
| `--output PATH` | `-o` | 写入文件而非 stdout |

### `ocs annotate <id> [text]`

| 选项 | 短名 | 说明 |
|------|------|------|
| `--remove` | `-r` | 删除注释 |

- 有 `text`：创建或更新注释
- 无 `text` 且无 `--remove`：查看现有注释
- 带 `--remove`：删除注释

### `ocs report`

| 选项 | 短名 | 说明 |
|------|------|------|
| `--since YYYY-MM-DD` | `-s` | 开始日期 |
| `--until YYYY-MM-DD` | `-u` | 结束日期（默认今天） |
| `--project DIR` | `-p` | 按项目目录筛选 |
| `--format FORMAT` | `-f` | 输出格式：`markdown`（默认）或 `json` |
| `--output PATH` | `-o` | 写入文件而非 stdout |

生成使用报告：每日趋势（ASCII 柱状图）、模型分布、费用汇总。

### `ocs export <id>`

| 选项 | 短名 | 说明 |
|------|------|------|
| `--format FORMAT` | `-f` | 输出格式：`markdown`（默认）或 `json` |
| `--output PATH` | `-o` | 输出文件路径（默认 `<title>-<short_id>.<ext>`） |
| `--no-tool` | `-n` | 隐藏工具调用详情 |
| `--sanitize` | `-s` | 脱敏敏感数据 |

### `ocs watch [id]`

| 选项 | 短名 | 说明 |
|------|------|------|
| `--poll SECS` | `-p` | 轮询间隔秒数（默认 2） |
| `--no-tool` | `-n` | 隐藏工具调用详情 |
| `--compact` | `-c` | 紧凑输出模式 |

轮询数据库获取新消息，增量渲染。

### `ocs tag [id] [tag]`

| 选项 | 短名 | 说明 |
|------|------|------|
| `--remove` | `-r` | 从会话移除标签 |
| `--list` | `-l` | 列出所有标签及使用次数 |
| `--search TAG` | `-s` | 按标签搜索会话 |

### `ocs context <id>`

显示会话概要：元数据、标签、注释、Token 用量、代理、同项目相关会话。

### `ocs completion <shell>`

生成 Shell 补全脚本。

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

| 选项 | 短名 | 说明 |
|------|------|------|
| `--rebuild` | `-r` | 从头重建索引 |
| `--status` | `-s` | 查看索引状态（不重建） |

管理 `~/.local/share/opencode/ocs_fts.db` 中的 FTS5 全文搜索索引。首次搜索时自动升级创建。

### `ocs autotag <action>`

操作：`list`（列出规则）、`add`（添加规则）、`remove`（删除规则）、`apply`（应用规则）

| 选项 | 短名 | 说明 |
|------|------|------|
| `--tag NAME` | `-t` | 标签名（用于 add/remove） |
| `--title-contains TEXT` | `-T` | 标题包含文本（规则条件） |
| `--dir-contains TEXT` | `-d` | 目录包含文本 |
| `--model NAME` | `-m` | 模型名称匹配 |
| `--min-cost N` | | 最小费用阈值 |
| `--max-cost N` | | 最大费用阈值 |
| `--min-msgs N` | | 最少消息数 |
| `--max-msgs N` | | 最多消息数 |
| `--all` | `-a` | 应用到所有会话（不限未打标签的） |

```bash
# 添加规则：费用 > $1 时自动打 "high-cost" 标签
ocs autotag add --tag high-cost --min-cost 1.0

# 应用所有规则
ocs autotag apply

# 列出规则
ocs autotag list

# 删除规则
ocs autotag remove --tag high-cost
```

## Fish Shell 会话 ID 补全

将以下内容添加到 `~/.config/fish/config.fish`，即可在输入会话 ID 时按 TAB 补全：

```fish
function __ocs_list_ids
    ocs list --compact 2>/dev/null | string replace -r '\t.*' ''
end

# 以会话 ID 为位置参数的命令
complete -c ocs -n "__fish_seen_subcommand_from show stats diff annotate export" -xa "(__ocs_list_ids)"
complete -c ocs -n "__fish_seen_subcommand_from context watch" -xa "(__ocs_list_ids)"
complete -c ocs -n "__fish_seen_subcommand_from rename" -n "__fish_is_nth_token 1" -xa "(__ocs_list_ids)"

# run --session / -s 标志
complete -c ocs -n "__fish_seen_subcommand_from run" -s s -l session -xa "(__ocs_list_ids)"
```

添加后执行 `source ~/.config/fish/config.fish` 即可生效。之后 `ocs show <TAB>`、`ocs run -s <TAB>` 等都会弹出会话 ID 的模糊补全列表。

## 数据来源

ocs 直接读取：

```
~/.local/share/opencode/opencode.db
```

数据库以只读方式访问（使用 `PRAGMA query_only=ON`）。写操作（`rename`、`prune`、`index`、`autotag`）会打开单独的读写连接。

### 表结构

| 表 | 内容 |
|-------|------|
| `session` | 会话元数据（id、标题、目录、时间戳、文件变更摘要） |
| `message` | 聊天消息，JSON `data` 列（role、agent、model、tokens、cost） |
| `part` | 消息片段，JSON `data` 列（文本、工具调用、推理过程、步骤信息） |

文件差异存储在 `~/.local/share/opencode/storage/session_diff/<id>.json`。

元数据（标签、注释、自动标签规则）存储在 `~/.local/share/opencode/ocs_meta.json`。

## 架构

```
src/
├── main.rs      # CLI 入口，clap 参数解析，命令分发（~1400 行）
├── db.rs        # SQLite 查询（列表、获取、搜索、统计、排行、项目、清理、FTS5 等）
├── models.rs    # 数据结构（Session、Message、Part + 结果类型）
├── render.rs    # Markdown/紧凑渲染（列表、详情、统计、搜索、差异、报告等）
└── meta.rs      # JSON sidecar 管理器（标签、注释、自动标签规则、原子写入）
```

依赖：
- `rusqlite` — 系统 SQLite 绑定
- `clap` — 命令行参数解析
- `clap_complete` — Shell 补全生成
- `serde_json` — 消息/片段数据的 JSON 反序列化
- `chrono` — 时间戳格式化
- `anyhow` — 错误处理
- `dialoguer` — 交互式模糊选择器和确认提示
- `ctrlc` — watch 模式的信号处理
