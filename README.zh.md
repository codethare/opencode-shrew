# ocs — OpenCode 会话查看器

直接从 SQLite 读取和浏览 [OpenCode](https://github.com/opencode-ai/opencode) 会话数据，绕过 `opencode export`。

## 功能

- **列表** 会话，支持筛选（日期范围、搜索、项目目录）
- **查看** 会话对话，支持 markdown 渲染、推理过程展开、工具调用详情
- **搜索** 所有消息全文搜索，附带上下文片段
- **统计** 每会话 Token 统计、费用、按代理细分
- **排行** 按费用、Token、消息量排名会话
- **项目** 按工作目录分组查看会话数量
- **差异** 查看会话期间的文件变更
- **重命名** 无需启动 TUI 即可重命名会话
- **清理** 支持 dry-run 和确认提示的旧会话清理
- **运行** session 感知的 `opencode run` 封装，自动从数据库读取 `--dir`

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
ocs <COMMAND>

Commands:
  list      列出会话
  show      查看会话消息
  stats     查看会话聚合统计
  top       按费用/Token/消息量排行
  search    全文搜索会话内容
  projects  列出项目目录及会话数
  rename    重命名会话
  prune     清理旧会话
  diff      查看会话文件差异
  run       session 感知的 opencode 子进程封装
  help      打印帮助信息
```

### `ocs list`

| 选项 | 说明 |
|--------|-------------|
| `-l`, `--limit N` | 最大显示数（默认 20，0 为全部） |
| `-s`, `--search TERM` | 按标题或 ID 筛选 |
| `--since YYYY-MM-DD` | 此日期之后创建的会话 |
| `--until YYYY-MM-DD` | 此日期之前创建的会话 |
| `--project DIR` | 按项目目录筛选（部分匹配） |
| `--compact` | Tab 分隔的紧凑格式 |
| `-i`, `--interactive` | 通过 peco/fzf 交互式选择 |

### `ocs show <id>`

| 选项 | 说明 |
|--------|-------------|
| `--raw` | 输出原始 JSON |
| `--no-tool` | 隐藏工具调用详情 |

### `ocs stats <id>`

显示消息数、Token 用量（输入/输出/推理/缓存）、总费用和按代理细分。

### `ocs top`

| 选项 | 说明 |
|--------|-------------|
| `-l`, `--limit N` | 最大显示数（默认 10） |
| `-b`, `--by FIELD` | 排序字段：`cost`（默认）、`tokens`、`msgs` |

### `ocs search <query>`

| 选项 | 说明 |
|--------|-------------|
| `-l`, `--limit N` | 最大匹配数（默认 20） |

### `ocs projects`

| 选项 | 说明 |
|--------|-------------|
| `-l`, `--limit N` | 最大项目数（默认 20） |

### `ocs rename <id> <new-title>`

原地重命名会话（更新标题）。

### `ocs prune`

| 选项 | 说明 |
|--------|-------------|
| `-d`, `--older-than DAYS` | 删除 N 天前的会话（默认 30） |
| `--dry-run` | 预览要删除的内容，不实际删除 |
| `--force` | 跳过确认提示 |

### `ocs run <message>`

`opencode run` 的 session 感知封装。继承 stdin/stdout 用于交互式使用。

| 选项 | 说明 |
|--------|-------------|
| `-s`, `--session ID` | 继续此会话（自动从数据库设置 `--dir`） |
| `-f`, `--fork` | 从指定会话派生 |
| `-i`, `--interactive` | 通过 peco/fzf 交互式选择会话 |

当指定 `--session` 或 `-i` 时，ocs 从 SQLite 查找会话的工作目录，并自动传递 `--dir <path>` 给 `opencode run`。

### `ocs diff <id>`

显示会话期间记录的文件变更（unified diff 格式）。

## 示例

```bash
# 列出最近会话
ocs list

# 按项目筛选
ocs list --project omocode

# 紧凑格式，管道到 peco
ocs list --compact -i

# 查看会话
ocs show ses_abc123

# 查看时不显示工具调用细节
ocs show ses_abc123 --no-tool

# 跨会话全文搜索
ocs search "error handling"

# 查看费用最高的会话
ocs top --by cost --limit 5

# 查看会话统计
ocs stats ses_abc123

# 查看有哪些项目
ocs projects

# 重命名会话
ocs rename ses_abc123 "My new title"

# 预览 60 天前的旧会话
ocs prune --older-than 60 --dry-run

# 删除它们
ocs prune --older-than 60 --force

# 查看文件差异
ocs diff ses_abc123

# 在已有会话中继续（自动从数据库设置 --dir）
ocs run -s ses_abc123 "continue implementing this feature"

# 从会话派生，尝试不同方案
ocs run -s ses_abc123 --fork "try a different approach"

# 交互式：通过 peco/fzf 选择会话后执行
ocs run -i "从选中的会话继续"
```

## 数据来源

ocs 直接读取：

```
~/.local/share/opencode/opencode.db
```

数据库以只读方式访问（使用 `PRAGMA query_only=ON`）。写操作（`rename`、`prune`）会打开单独的读写连接。

### 表结构

| 表 | 内容 |
|-------|----------|
| `session` | 会话元数据（id、标题、目录、时间戳、文件变更摘要） |
| `message` | 聊天消息，JSON `data` 列（role、agent、model、tokens、cost） |
| `part` | 消息片段，JSON `data` 列（文本、工具调用、推理过程、步骤信息） |

文件差异存储在 `~/.local/share/opencode/storage/session_diff/<id>.json`。

## 架构

```
src/
├── main.rs      # CLI 入口，clap 参数解析，命令分发
├── db.rs        # SQLite 查询（列表、获取、搜索、统计、排行、项目、清理）
├── models.rs    # 数据结构（Session、Message、Part + 结果类型）
└── render.rs    # Markdown/紧凑渲染（列表、详情、统计、搜索、差异等）
```

依赖：
- `rusqlite` — 系统 SQLite 绑定
- `clap` — 命令行参数解析
- `serde_json` — 消息/片段数据的 JSON 反序列化
- `chrono` — 时间戳格式化
- `anyhow` — 错误处理
