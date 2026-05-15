mod db;
mod models;
mod render;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Parser)]
#[command(name = "ocv", version, about = "OpenCode Viewer - browse opencode sessions directly from SQLite")]
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
        #[arg(long)]
        project: Option<String>,

        /// Compact one-line format
        #[arg(long)]
        compact: bool,

        /// Interactive selection via peco/fzf
        #[arg(short, long)]
        interactive: bool,
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
    },

    /// Show aggregated stats for a session (tokens, cost, per-agent breakdown)
    Stats {
        /// Session ID
        id: String,
    },

    /// Search session content for a query string
    Search {
        /// Query string to search for in messages
        query: String,

        /// Maximum matches to show
        #[arg(short, long, default_value = "20")]
        limit: i64,
    },

    /// List project directories with session counts
    Projects {
        /// Maximum projects to show
        #[arg(short, long, default_value = "20")]
        limit: i64,
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
        /// Message or prompt to send
        message: String,

        /// Continue this session (auto-sets --dir from DB)
        #[arg(short, long)]
        session: Option<String>,

        /// Fork from the specified session
        #[arg(short, long)]
        fork: bool,

        /// Interactive mode: pick session from peco/fzf
        #[arg(short, long)]
        interactive: bool,
    },

    /// Show top sessions by cost, tokens, or message count
    Top {
        /// Maximum sessions to show
        #[arg(short, long, default_value = "10")]
        limit: i64,

        /// Sort by: cost (default), tokens, or msgs
        #[arg(short, long, default_value = "cost")]
        by: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let conn = db::open_db(None)?;

    match cli.command {
        Commands::List { limit, search, since, until, project, compact, interactive } => {
            cmd_list(&conn, limit, search.as_deref(), since.as_deref(), until.as_deref(), project.as_deref(), compact, interactive)
        }
        Commands::Show { id, raw, no_tool } => {
            cmd_show(&conn, &id, raw, !no_tool)
        }
        Commands::Stats { id } => {
            cmd_stats(&conn, &id)
        }
        Commands::Search { query, limit } => {
            cmd_search(&conn, &query, limit)
        }
        Commands::Projects { limit } => {
            cmd_projects(&conn, limit)
        }
        Commands::Rename { id, title } => {
            cmd_rename(&conn, &id, &title)
        }
        Commands::Prune { older_than, dry_run, force } => {
            cmd_prune(&conn, older_than, dry_run, force)
        }
        Commands::Diff { id } => {
            cmd_diff(&conn, &id)
        }
        Commands::Run { message, session, fork, interactive } => {
            cmd_run(&conn, &message, session.as_deref(), fork, interactive)
        }
        Commands::Top { limit, by } => {
            cmd_top(&conn, limit, &by)
        }
    }
}

fn cmd_list(
    conn: &rusqlite::Connection,
    limit: i64,
    search: Option<&str>,
    since: Option<&str>,
    until: Option<&str>,
    project: Option<&str>,
    compact: bool,
    interactive: bool,
) -> Result<()> {
    let since_ts = since.and_then(parse_date);
    let until_ts = until.and_then(parse_date);

    let sessions = db::list_sessions(conn, limit, search, since_ts, until_ts, project)?;

    if sessions.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }

    if interactive {
        return cmd_list_interactive(conn, &sessions);
    }

    if compact {
        let lines = render::render_session_list_compact(&sessions);
        for line in &lines {
            println!("{line}");
        }
    } else {
        let output = render::render_session_list(&sessions);
        println!("{output}");
    }
    Ok(())
}

/// Pick a session interactively via peco/fzf, returns the selected session ID
fn pick_session_interactive(_conn: &rusqlite::Connection, sessions: &[models::Session]) -> Result<String> {
    let lines = render::render_session_list_compact(sessions);

    // Try peco first, then fzf
    let selector = if which("peco").is_ok() {
        "peco"
    } else if which("fzf").is_ok() {
        "fzf"
    } else {
        eprintln!("Neither peco nor fzf found. Install one for interactive mode.");
        eprintln!("  cargo install peco  or  brew install fzf");
        std::process::exit(1);
    };

    let mut child = Command::new(selector)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("Failed to spawn {selector}"))?;

    {
        let stdin = child.stdin.as_mut().unwrap();
        for line in &lines {
            writeln!(stdin, "{line}")?;
        }
    }

    let output = child.wait_with_output()?;
    if !output.status.success() {
        // User cancelled (peco returns 1 on ESC)
        std::process::exit(1);
    }

    let selected = String::from_utf8_lossy(&output.stdout);
    selected.lines().next()
        .and_then(|line| line.split('\t').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .with_context(|| "No session selected.")
}

fn cmd_list_interactive(conn: &rusqlite::Connection, sessions: &[models::Session]) -> Result<()> {
    match pick_session_interactive(conn, sessions) {
        Ok(id) => cmd_show(conn, &id, false, true),
        Err(e) => {
            eprintln!("{e}");
            Ok(())
        }
    }
}

fn cmd_show(conn: &rusqlite::Connection, id: &str, raw: bool, show_tools: bool) -> Result<()> {
    let session = db::get_session(conn, id)?
        .with_context(|| format!("Session not found: {id}"))?;

    let messages = db::get_messages(conn, id)?;

    let msg_ids: Vec<String> = messages.iter().map(|m| m.id.clone()).collect();
    let parts_map = db::get_parts_batch(conn, &msg_ids)?;

    let mut messages_with_parts: Vec<models::MessageWithParts> = messages
        .into_iter()
        .map(|msg| {
            let parts = parts_map
                .iter()
                .find(|(mid, _)| *mid == msg.id)
                .map(|(_, parts)| parts.clone())
                .unwrap_or_default();
            models::MessageWithParts {
                message: msg,
                parts,
            }
        })
        .collect();

    messages_with_parts.retain(|m| {
        m.message.data.role == "user" || m.message.data.role == "assistant"
    });

    if raw {
        let json = render::render_session_raw(&session, &messages_with_parts)?;
        println!("{json}");
    } else {
        let output = render::render_session_detail(&session, &messages_with_parts, show_tools);
        println!("{output}");
    }

    Ok(())
}

fn cmd_rename(_conn: &rusqlite::Connection, id: &str, title: &str) -> Result<()> {
    // Open RW connection for write operations
    let rw_conn = db::open_db_rw(None)?;
    if db::rename_session(&rw_conn, id, title)? {
        println!("Session {id} renamed to: {title}");
    } else {
        eprintln!("Session not found: {id}");
    }
    Ok(())
}

fn cmd_prune(conn: &rusqlite::Connection, days: i64, dry_run: bool, force: bool) -> Result<()> {
    let sessions = db::list_old_sessions(conn, days, 100)?;

    if sessions.is_empty() {
        println!("No sessions older than {days} days found.");
        return Ok(());
    }

    let plan = render::render_prune_plan(&sessions, days);
    println!("{plan}");

    if dry_run {
        println!("Dry-run mode. No sessions were deleted.");
        return Ok(());
    }

    if !force {
        print!("Delete these {} session(s)? [y/N] ", sessions.len());
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();
        if input != "y" && input != "yes" {
            println!("Aborted.");
            return Ok(());
        }
    }

    let rw_conn = db::open_db_rw(None)?;
    let mut deleted = 0;
    let mut failed = 0;
    for s in &sessions {
        match db::delete_session(&rw_conn, &s.id) {
            Ok(true) => deleted += 1,
            Ok(false) => failed += 1,
            Err(e) => {
                eprintln!("Failed to delete {}: {e}", s.id);
                failed += 1;
            }
        }
    }
    println!("Deleted {deleted} session(s).", );
    if failed > 0 {
        eprintln!("{failed} session(s) could not be deleted.");
    }
    Ok(())
}

fn cmd_diff(conn: &rusqlite::Connection, id: &str) -> Result<()> {
    // Verify session exists
    let _session = db::get_session(conn, id)?
        .with_context(|| format!("Session not found: {id}"))?;

    let entries = db::read_session_diff(id)?;
    let output = render::render_diff(&entries, id);
    println!("{output}");
    Ok(())
}

fn cmd_search(conn: &rusqlite::Connection, query: &str, limit: i64) -> Result<()> {
    let results = db::search_sessions(conn, query, limit)?;
    let output = render::render_search_results(&results, query);
    println!("{output}");
    Ok(())
}

fn cmd_projects(conn: &rusqlite::Connection, limit: i64) -> Result<()> {
    let groups = db::list_projects(conn, limit)?;
    if groups.is_empty() {
        println!("No projects found.");
        return Ok(());
    }
    let output = render::render_projects(&groups);
    println!("{output}");
    Ok(())
}

fn cmd_stats(conn: &rusqlite::Connection, id: &str) -> Result<()> {
    let stats = db::get_session_stats(conn, id)?;
    let output = render::render_session_stats(&stats);
    println!("{output}");
    Ok(())
}

fn cmd_run(conn: &rusqlite::Connection, message: &str, session: Option<&str>, fork: bool, interactive: bool) -> Result<()> {
    if which("opencode").is_err() {
        eprintln!("Error: 'opencode' binary not found in PATH.");
        std::process::exit(1);
    }

    let sid = match session {
        Some(s) => s.to_string(),
        None if interactive => {
            let sessions = db::list_sessions(conn, 50, None, None, None, None)?;
            if sessions.is_empty() {
                eprintln!("No sessions found.");
                std::process::exit(1);
            }
            pick_session_interactive(conn, &sessions)?
        }
        None => String::new(),
    };

    let mut cmd = Command::new("opencode");
    cmd.arg("run");

    if !sid.is_empty() {
        cmd.arg("-s").arg(&sid);

        // Auto-set --dir from session's directory in DB
        if let Ok(Some(s)) = db::get_session(conn, &sid) {
            if !s.directory.is_empty() {
                cmd.arg("--dir").arg(&s.directory);
            }
        }
    }

    if fork {
        cmd.arg("--fork");
    }

    cmd.arg(message);

    cmd.stdin(Stdio::inherit());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());

    let status = cmd.status().with_context(|| "Failed to execute 'opencode run'")?;
    std::process::exit(status.code().unwrap_or(1));
}

fn cmd_top(conn: &rusqlite::Connection, limit: i64, by: &str) -> Result<()> {
    let valid_by = match by {
        "cost" | "tokens" | "msgs" => by,
        _ => {
            eprintln!("Invalid sort: '{by}'. Use 'cost', 'tokens', or 'msgs'.");
            std::process::exit(1);
        }
    };
    let entries = db::top_sessions(conn, limit, valid_by)?;
    if entries.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }
    let output = render::render_top_sessions(&entries, valid_by);
    println!("{output}");
    Ok(())
}

fn parse_date(s: &str) -> Option<i64> {
    // Try YYYY-MM-DD HH:MM:SS
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Some(dt.and_utc().timestamp_millis());
    }
    // Try YYYY-MM-DD (start of day)
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let dt = d.and_hms_opt(0, 0, 0).unwrap();
        return Some(dt.and_utc().timestamp_millis());
    }
    eprintln!("Warning: could not parse date '{s}'. Expected YYYY-MM-DD or YYYY-MM-DD HH:MM:SS");
    None
}

fn which(name: &str) -> std::io::Result<std::process::Output> {
    Command::new("which").arg(name).output()
}
