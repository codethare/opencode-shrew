mod cmd;
mod db;
mod meta;
mod models;
mod pager;
mod render;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ocs", version, about = "OpenCode session viewer - browse opencode sessions directly from SQLite")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List sessions
    List {
        /// Maximum sessions to show (0 = all)
        #[arg(short, long, default_value = "20")]
        limit: i64,

        /// Filter sessions by title or ID
        #[arg(short, long)]
        search: Option<String>,

        /// Show sessions created after this date (YYYY-MM-DD)
        #[arg(long)]
        since: Option<String>,

        /// Show sessions created before this date (YYYY-MM-DD)
        #[arg(long)]
        until: Option<String>,

        /// Filter by project directory (partial path match)
        #[arg(short = 'p', long)]
        project: Option<String>,

        /// Compact one-line format
        #[arg(long)]
        compact: bool,

        /// Interactive selection via peco/fzf
        #[arg(short, long)]
        interactive: bool,

        /// Output as JSON
        #[arg(short = 'j', long)]
        json: bool,

        /// Only show sessions with annotations
        #[arg(long)]
        annotated: bool,

        /// Only show sessions submitted from the command line (exclude search results, community questions, etc.)
        #[arg(long)]
        cli_only: bool,

        /// Show all sessions, including subagent background sessions
        #[arg(long)]
        all: bool,

        /// Minimum number of messages (filter out empty/failed sessions)
        #[arg(long)]
        min_msgs: Option<i64>,
    },

    /// Show a session's messages
    Show {
        /// Session ID (e.g. ses_abc123)
        id: String,

        /// Output raw JSON instead of formatted markdown
        #[arg(long)]
        raw: bool,

        /// Hide tool call details
        #[arg(long)]
        no_tool: bool,

        /// Redact sensitive data in output
        #[arg(long)]
        sanitize: bool,
    },

    /// Show aggregated stats for a session (tokens, cost, per-agent breakdown)
    Stats {
        /// Session ID
        id: String,

        /// Output as JSON
        #[arg(short = 'j', long)]
        json: bool,
    },

    /// Search session content for a query string
    Search {
        /// Query string to search for in messages
        query: String,

        /// Maximum matches to show
        #[arg(short, long, default_value = "20")]
        limit: i64,

        /// Output as JSON
        #[arg(short = 'j', long)]
        json: bool,
    },

    /// List project directories with session counts
    Projects {
        /// Maximum projects to show
        #[arg(short, long, default_value = "20")]
        limit: i64,

        /// Output as JSON
        #[arg(short = 'j', long)]
        json: bool,
    },

    /// Rename a session
    Rename {
        /// Session ID
        id: String,
        /// New title
        title: String,
    },

    /// Prune old sessions
    Prune {
        /// Delete sessions older than this many days
        #[arg(short, long, default_value = "30")]
        older_than: i64,

        /// Show what would be deleted without deleting
        #[arg(long)]
        dry_run: bool,

        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Show file diffs for a session
    Diff {
        /// Session ID
        id: String,
    },

    /// Run opencode as a subprocess (session-aware wrapper around `opencode run`)
    Run {
        /// Message or prompt to send (omit for interactive input, or use --input/-I for dialoguer prompt)
        message: Option<String>,

        /// Continue this session (auto-sets --dir from DB)
        #[arg(short, long)]
        session: Option<String>,

        /// Fork from the specified session
        #[arg(short, long)]
        fork: bool,

        /// Interactive mode: pick session from peco/fzf
        #[arg(short, long)]
        interactive: bool,

        /// Open interactive prompt to compose the message
        #[arg(short = 'I', long)]
        input: bool,

        /// Continue the most recently updated session (maps to opencode -c)
        #[arg(short = 'c', long = "continue")]
        continue_flag: bool,

        /// Open $EDITOR/nvim to compose the message
        #[arg(short = 'e', long)]
        edit: bool,

        /// Filter sessions by project directory when using -c or -i
        #[arg(short = 'p', long)]
        project: Option<String>,
    },

    /// Show top sessions by cost, tokens, or message count
    Top {
        /// Maximum sessions to show
        #[arg(short, long, default_value = "10")]
        limit: i64,

        /// Sort by: cost (default), tokens, or msgs
        #[arg(short, long, default_value = "cost")]
        by: String,

        /// Output as JSON
        #[arg(short = 'j', long)]
        json: bool,
    },

    /// Compare two sessions side by side
    Compare {
        /// First session ID
        id1: String,
        /// Second session ID
        id2: String,
        /// Show only stats comparison (skip message alignment)
        #[arg(long)]
        stats: bool,
        /// Output as JSON
        #[arg(short = 'j', long)]
        json: bool,
        /// Output file path
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Add or view annotations on a session
    Annotate {
        /// Session ID
        id: String,

        /// Annotation text (omit to view, use --remove to delete)
        text: Option<String>,

        /// Remove the annotation
        #[arg(long)]
        remove: bool,
    },

    /// Generate a usage report for sessions in a date range
    Report {
        /// Start date (YYYY-MM-DD)
        #[arg(long)]
        since: Option<String>,

        /// End date (YYYY-MM-DD, default: today)
        #[arg(long)]
        until: Option<String>,

        /// Filter by project directory
        #[arg(short = 'p', long)]
        project: Option<String>,

        /// Output format: markdown or json
        #[arg(short, long, default_value = "markdown")]
        format: String,

        /// Output file path
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Export a session to a file
    Export {
        /// Session ID
        id: String,

        /// Output format: markdown or json
        #[arg(short, long, default_value = "markdown")]
        format: String,

        /// Output file path (default: <title>-<short_id>.<ext>)
        #[arg(short, long)]
        output: Option<String>,

        /// Hide tool call details
        #[arg(long)]
        no_tool: bool,

        /// Redact sensitive data in output
        #[arg(long)]
        sanitize: bool,
    },

    /// Watch a session in real-time (poll for new messages)
    Watch {
        /// Session ID to watch (default: latest active session)
        id: Option<String>,

        /// Poll interval in seconds
        #[arg(short, long, default_value = "2")]
        poll: u64,

        /// Hide tool call details
        #[arg(long)]
        no_tool: bool,

        /// Compact output mode
        #[arg(long)]
        compact: bool,
    },

    /// Manage session tags
    Tag {
        /// Session ID
        id: Option<String>,

        /// Tag to add or remove
        tag: Option<String>,

        /// Remove a tag from a session
        #[arg(long)]
        remove: bool,

        /// List all tags with usage counts
        #[arg(long)]
        list: bool,

        /// Search sessions by tag
        #[arg(long)]
        search: Option<String>,
    },

    /// Undo a message in a session (interactive: select session + message, then [d]elete or [e]dit)
    Undo {
        /// Use the most recent session
        #[arg(short = 'c', long = "continue")]
        continue_flag: bool,

        /// Session ID
        #[arg(short, long)]
        session: Option<String>,

        /// Filter sessions by project directory when using -c or interactive mode
        #[arg(short = 'p', long)]
        project: Option<String>,
    },

    /// Generate shell completion scripts
    Completion {
        /// Shell type: bash, zsh, fish, powershell, elvish
        shell: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let conn = db::open_db(None)?;

    match cli.command {
        Commands::List { limit, search, since, until, project, compact, interactive, json, annotated, cli_only, all, min_msgs } => {
            cmd::cmd_list(&conn, limit, search.as_deref(), since.as_deref(), until.as_deref(), project.as_deref(), compact, interactive, json, annotated, cli_only, all, min_msgs)
        }
        Commands::Show { id, raw, no_tool, sanitize } => {
            cmd::cmd_show(&conn, &id, raw, !no_tool, sanitize)
        }
        Commands::Stats { id, json } => {
            cmd::cmd_stats(&conn, &id, json)
        }
        Commands::Search { query, limit, json } => {
            cmd::cmd_search(&conn, &query, limit, json)
        }
        Commands::Projects { limit, json } => {
            cmd::cmd_projects(&conn, limit, json)
        }
        Commands::Rename { id, title } => {
            cmd::cmd_rename(&conn, &id, &title)
        }
        Commands::Prune { older_than, dry_run, force } => {
            cmd::cmd_prune(&conn, older_than, dry_run, force)
        }
        Commands::Diff { id } => {
            cmd::cmd_diff(&conn, &id)
        }
        Commands::Run { message, session, fork, interactive, input, continue_flag, edit, project } => {
            cmd::cmd_run(&conn, message.as_deref(), session.as_deref(), fork, interactive, input, continue_flag, edit, project.as_deref())
        }
        Commands::Top { limit, by, json } => {
            cmd::cmd_top(&conn, limit, &by, json)
        }
        Commands::Compare { id1, id2, stats, json, output } => {
            cmd::cmd_compare(&conn, &id1, &id2, stats, json, output.as_deref())
        }
        Commands::Annotate { id, text, remove } => {
            cmd::cmd_annotate(&conn, &id, text.as_deref(), remove)
        }
        Commands::Report { since, until, project, format, output } => {
            cmd::cmd_report(&conn, since.as_deref(), until.as_deref(), project.as_deref(), &format, output.as_deref())
        }
        Commands::Export { id, format, output, no_tool, sanitize } => {
            cmd::cmd_export(&conn, &id, &format, output.as_deref(), !no_tool, sanitize)
        }
        Commands::Watch { id, poll, no_tool, compact } => {
            cmd::cmd_watch(&conn, id.as_deref(), poll, !no_tool, compact)
        }
        Commands::Tag { id, tag, remove, list, search } => {
            cmd::cmd_tag(&conn, id.as_deref(), tag.as_deref(), remove, list, search.as_deref())
        }
        Commands::Undo { session, continue_flag, project } => {
            cmd::cmd_undo(&conn, session.as_deref(), continue_flag, project.as_deref())
        }
        Commands::Completion { shell } => {
            cmd::cmd_completion(&shell)
        }
    }
}
