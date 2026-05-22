mod db;
mod meta;
mod models;
mod pager;
mod render;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use clap_complete::generate;
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

/// Set to true when we intentionally exit after subprocess completion
static SUBPROCESS_EXIT: AtomicBool = AtomicBool::new(false);

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
        #[arg(long)]
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
        #[arg(long)]
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
            cmd_list(&conn, limit, search.as_deref(), since.as_deref(), until.as_deref(), project.as_deref(), compact, interactive, json, annotated, cli_only, all, min_msgs)
        }
        Commands::Show { id, raw, no_tool, sanitize } => {
            cmd_show(&conn, &id, raw, !no_tool, sanitize)
        }
        Commands::Stats { id, json } => {
            cmd_stats(&conn, &id, json)
        }
        Commands::Search { query, limit, json } => {
            cmd_search(&conn, &query, limit, json)
        }
        Commands::Projects { limit, json } => {
            cmd_projects(&conn, limit, json)
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
        Commands::Run { message, session, fork, interactive, input, continue_flag, edit, project } => {
            cmd_run(&conn, message.as_deref(), session.as_deref(), fork, interactive, input, continue_flag, edit, project.as_deref())
        }
        Commands::Top { limit, by, json } => {
            cmd_top(&conn, limit, &by, json)
        }
        Commands::Compare { id1, id2, stats, json, output } => {
            cmd_compare(&conn, &id1, &id2, stats, json, output.as_deref())
        }
        Commands::Annotate { id, text, remove } => {
            cmd_annotate(&conn, &id, text.as_deref(), remove)
        }
        Commands::Report { since, until, project, format, output } => {
            cmd_report(&conn, since.as_deref(), until.as_deref(), project.as_deref(), &format, output.as_deref())
        }
        Commands::Export { id, format, output, no_tool, sanitize } => {
            cmd_export(&conn, &id, &format, output.as_deref(), !no_tool, sanitize)
        }
        Commands::Watch { id, poll, no_tool, compact } => {
            cmd_watch(&conn, id.as_deref(), poll, !no_tool, compact)
        }
        Commands::Tag { id, tag, remove, list, search } => {
            cmd_tag(&conn, id.as_deref(), tag.as_deref(), remove, list, search.as_deref())
        }
        Commands::Undo { session, continue_flag, project } => {
            cmd_undo(&conn, session.as_deref(), continue_flag, project.as_deref())
        }
        Commands::Completion { shell } => {
            cmd_completion(&shell)
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
    json: bool,
    annotated: bool,
    cli_only: bool,
    show_all: bool,
    min_msgs: Option<i64>,
) -> Result<()> {
    let since_ts = since.and_then(parse_date);
    let until_ts = until.and_then(parse_date);

    let sessions = if annotated {
        let annotated_ids = meta::list_annotated_ids()?;
        if annotated_ids.is_empty() {
            Vec::new()
        } else {
            db::list_sessions_by_ids(conn, &annotated_ids)?
        }
    } else {
        db::list_sessions(conn, limit, search, since_ts, until_ts, project, min_msgs, cli_only, show_all)?
    };

    if sessions.is_empty() {
        if json {
            println!("[]");
        } else {
            println!("No sessions found.");
        }
        return Ok(());
    }

    if json {
        let output = serde_json::to_string_pretty(&sessions)?;
        println!("{output}");
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

    let selector = if which("peco").is_ok() {
        "peco"
    } else if which("fzf").is_ok() {
        "fzf"
    } else {
        bail!("Neither peco nor fzf found. Install one for interactive mode.");
    };

    let mut child = Command::new(selector)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("Failed to spawn {selector}"))?;

    if let Some(stdin) = child.stdin.as_mut() {
        for line in &lines {
            writeln!(stdin, "{line}")?;
        }
    }

    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!("Selection cancelled.");
    }

    let selected = String::from_utf8(output.stdout)
        .with_context(|| "peco/fzf output contained invalid UTF-8")?;
    selected.lines().next()
        .and_then(|line| line.split('\t').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .with_context(|| "No session selected.")
}

fn cmd_list_interactive(conn: &rusqlite::Connection, sessions: &[models::Session]) -> Result<()> {
    match pick_session_interactive(conn, sessions) {
        Ok(id) => cmd_show(conn, &id, false, true, false),
        Err(e) => {
            eprintln!("{e}");
            Ok(())
        }
    }
}

fn validate_session_id(id: &str) -> Result<()> {
    if id.contains("..") || id.contains('/') || id.contains('\0') {
        bail!("Invalid session ID: path traversal detected");
    }
    Ok(())
}

fn cmd_show(conn: &rusqlite::Connection, id: &str, raw: bool, show_tools: bool, sanitize: bool) -> Result<()> {
    validate_session_id(id)?;
    let session = db::get_session(conn, id)?
        .with_context(|| format!("Session not found: {id}"))?;

    let note = match meta::get_note(id) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("Warning: could not read annotation: {e}");
            None
        }
    };

    if raw {
        let messages = db::get_messages(conn, id)?;
        let msg_ids: Vec<String> = messages.iter().map(|m| m.id.clone()).collect();
        let parts_map = db::get_parts_batch(conn, &msg_ids)?;
        let messages_with_parts: Vec<models::MessageWithParts> = messages
            .into_iter()
            .map(|msg| {
                let parts = parts_map
                    .iter()
                    .find(|(mid, _)| **mid == msg.id)
                    .map(|(_, parts)| parts.clone())
                    .unwrap_or_default();
                models::MessageWithParts { message: msg, parts }
            })
            .collect();
        let output = render::render_session_raw(&session, &messages_with_parts, sanitize)?;
        println!("{output}");
        return Ok(());
    }

    let total = db::get_message_count(conn, id)?;
    if total > 20 {
        return interactive_pager(conn, &session, show_tools, note);
    }

    let messages = db::get_messages(conn, id)?;
    let msg_ids: Vec<String> = messages.iter().map(|m| m.id.clone()).collect();
    let parts_map = db::get_parts_batch(conn, &msg_ids)?;
    let messages_with_parts: Vec<models::MessageWithParts> = messages
        .into_iter()
        .map(|msg| {
            let parts = parts_map
                .iter()
                .find(|(mid, _)| **mid == msg.id)
                .map(|(_, parts)| parts.clone())
                .unwrap_or_default();
            models::MessageWithParts { message: msg, parts }
        })
        .collect();

    let output = render::render_session_detail(&session, &messages_with_parts, show_tools);
    if let Some(ref n) = note {
        println!("📝 *Annotation*: {}\n{}\n", n, "-".repeat(60));
    }
    let rendered = render::render_markdown(&output);
    println!("{rendered}");
    Ok(())
}

fn cmd_rename(_conn: &rusqlite::Connection, id: &str, title: &str) -> Result<()> {
    validate_session_id(id)?;
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
    validate_session_id(id)?;
    let _session = db::get_session(conn, id)?
        .with_context(|| format!("Session not found: {id}"))?;

    let entries = db::read_session_diff(id)?;
    let output = render::render_diff(&entries, id);
    println!("{output}");
    Ok(())
}

fn cmd_search(conn: &rusqlite::Connection, query: &str, limit: i64, json: bool) -> Result<()> {
    let results = db::search_sessions(conn, query, limit)?;
    if json {
        let output = serde_json::to_string_pretty(&results)?;
        println!("{output}");
    } else {
        let output = render::render_search_results(&results, query);
        println!("{output}");
    }
    Ok(())
}

fn cmd_projects(conn: &rusqlite::Connection, limit: i64, json: bool) -> Result<()> {
    let groups = db::list_projects(conn, limit)?;
    if groups.is_empty() {
        if json {
            println!("[]");
        } else {
            println!("No projects found.");
        }
        return Ok(());
    }
    if json {
        let output = serde_json::to_string_pretty(&groups)?;
        println!("{output}");
    } else {
        let output = render::render_projects(&groups);
        println!("{output}");
    }
    Ok(())
}

fn cmd_stats(conn: &rusqlite::Connection, id: &str, json: bool) -> Result<()> {
    validate_session_id(id)?;
    let stats = db::get_session_stats(conn, id)?;
    if json {
        let output = serde_json::to_string_pretty(&stats)?;
        println!("{output}");
    } else {
        let output = render::render_session_stats(&stats);
        println!("{output}");
    }
    Ok(())
}

fn cmd_run(conn: &rusqlite::Connection, message: Option<&str>, session: Option<&str>, fork: bool, interactive: bool, input: bool, cont: bool, edit: bool, project: Option<&str>) -> Result<()> {
    if which("opencode").is_err() {
        bail!("'opencode' binary not found in PATH.");
    }

    let sid = match session {
        Some(s) => {
            validate_session_id(s)?;
            s.to_string()
        }
        None if interactive => {
            let sessions = db::list_sessions(conn, 50, None, None, None, project, None, false, true)?;
            if sessions.is_empty() {
                bail!("No sessions found.");
            }
            pick_session_interactive(conn, &sessions)?
        }
        None => String::new(),
    };

    let mut cmd = Command::new("opencode");
    cmd.arg("run");

    if !sid.is_empty() {
        cmd.arg("-s").arg(&sid);

        if let Ok(Some(s)) = db::get_session(conn, &sid) {
            if !s.directory.is_empty() {
                // Validate directory from DB before passing to subprocess
                if s.directory.contains('\0') || s.directory.len() > 4096 {
                    eprintln!("Warning: invalid directory in DB for session {sid}, skipping --dir");
                } else {
                    cmd.arg("--dir").arg(&s.directory);
                }
            }
        }
    }

    if fork {
        cmd.arg("--fork");
    }

    if cont {
        cmd.arg("-c");
    }

    let mut current_sid = sid;
    let mut first_run = true;

    loop {
        let prompt = if first_run {
            match message {
                Some(msg) => Some(msg.to_string()),
                None if edit => {
                    let p = read_editor_input()?;
                    if p.trim().is_empty() { None } else { Some(p) }
                }
                None if input => {
                    let p = read_multiline_input()?;
                    if p.trim().is_empty() { None } else { Some(p) }
                }
                _ => None,
            }
        } else if input {
            let p = read_multiline_input()?;
            if p.trim().is_empty() { None } else { Some(p) }
        } else {
            None
        };

        let Some(p) = prompt else {
            cmd.stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit());
            let status = cmd.status().with_context(|| "Failed to execute 'opencode run'")?;
            SUBPROCESS_EXIT.store(true, Ordering::Relaxed);
            std::process::exit(status.code().unwrap_or(1));
        };

        let mut iter_cmd = Command::new("opencode");
        iter_cmd.arg("run");

        if !current_sid.is_empty() {
            iter_cmd.arg("-s").arg(&current_sid);
            if let Ok(Some(s)) = db::get_session(conn, &current_sid) {
                if !s.directory.is_empty() {
                    if s.directory.contains('\0') || s.directory.len() > 4096 {
                        eprintln!("Warning: invalid directory in DB for session {current_sid}, skipping --dir");
                    } else {
                        iter_cmd.arg("--dir").arg(&s.directory);
                    }
                }
            }
        } else if cont || !first_run {
            iter_cmd.arg("-c");
        }

        if fork {
            iter_cmd.arg("--fork");
        }

        // Pipe prompt via stdin to prevent subagent cancellation.
        let mut child = iter_cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| "Failed to execute 'opencode run'")?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(p.as_bytes())?;
        }

        let mut output = Vec::new();
        if let Some(out) = child.stdout.take() {
            let mut limited = out.take(10 * 1024 * 1024);
            limited.read_to_end(&mut output)?;
        }
        let status = child.wait()?;

        let raw_str = String::from_utf8_lossy(&output);
        let cleaned = render::strip_ansi(&raw_str);
        let rendered = if cleaned.len() > 1_000_000 {
            let cutoff = cleaned[..1_000_000].rfind('\n').unwrap_or(1_000_000);
            let mut r = render::render_markdown(&cleaned[..cutoff]);
            r.push_str(&format!(
                "\n\x1b[2m... (output truncated at ~{}KB for rendering speed)\x1b[0m\n",
                cutoff / 1024,
            ));
            r
        } else {
            render::render_markdown(&cleaned)
        };
        let (_, term_height) = crossterm::terminal::size().unwrap_or((80, 24));
        let visible_h = term_height.saturating_sub(1) as usize;
        let lines: Vec<&str> = rendered.lines().collect();

        if lines.is_empty() || lines.len() <= visible_h {
            print!("{rendered}");
        } else {
            let owned: Vec<String> = lines.into_iter().map(|l| l.to_string()).collect();
            run_text_pager(&owned)?;
        }

        if first_run && current_sid.is_empty() {
            let recent = db::list_sessions(conn, 1, None, None, None, None, None, false, true)
                .unwrap_or_default();
            if let Some(s) = recent.first() {
                current_sid = s.id.clone();
            }
        }
        first_run = false;

        if input {
            use std::io::Write as _;
            let mut line = String::new();
            print!("\n\x1b[2m[u]ndo last msg  [Enter] next msg  [q]uit:\x1b[0m ");
            std::io::stdout().flush()?;
            std::io::stdin().read_line(&mut line)?;
            match line.trim().to_lowercase().as_str() {
                "u" => {
                    if let Ok(rw) = db::open_db_rw(None) {
                        if let Ok(msgs) = db::get_messages(&rw, &current_sid) {
                            if let Some(last) = msgs.iter().rev().find(|m| m.data.role == "user") {
                                let _ = rw.execute(
                                    "DELETE FROM part WHERE message_id = ?1",
                                    rusqlite::params![last.id],
                                );
                                let _ = rw.execute(
                                    "DELETE FROM message WHERE id = ?1",
                                    rusqlite::params![last.id],
                                );
                                println!("\x1b[2m✓ Undone. Type replacement below:\x1b[0m");
                            }
                        }
                    }
                    continue;
                }
                "" => {
                    continue;
                }
                _ => {}
            }
        }

        SUBPROCESS_EXIT.store(true, Ordering::Relaxed);
        std::process::exit(status.code().unwrap_or(1));
    }
}

fn redraw_input(stdout: &mut std::io::Stdout, buffer: &str) -> std::io::Result<()> {
    use crossterm::cursor;
    use crossterm::execute;
    use crossterm::terminal::{Clear, ClearType};
    execute!(stdout, cursor::RestorePosition, Clear(ClearType::FromCursorDown))?;
    // Raw mode: \n alone doesn't return to column 0. Use \r\n for newlines.
    for line in buffer.lines() {
        write!(stdout, "{line}\r\n")?;
    }
    // If buffer ends with \n, the loop above would have emitted an extra \r\n
    // for the empty segment. But lines() strips the trailing delimiter, so we
    // need to detect a trailing \n and emit one more \r\n.
    if buffer.ends_with('\n') {
        write!(stdout, "\r\n")?;
    }
    stdout.flush()
}

fn read_editor_input() -> Result<String> {
    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "vi".to_string());
    let tmp = format!("/tmp/ocs_prompt_{}.md", std::process::id());
    std::fs::write(&tmp, "")?;
    let status = std::process::Command::new(&editor)
        .arg(&tmp)
        .status()
        .with_context(|| format!("Failed to launch editor '{editor}'"))?;
    if !status.success() {
        let _ = std::fs::remove_file(&tmp);
        bail!("Editor exited unsuccessfully.");
    }
    let content = std::fs::read_to_string(&tmp)?;
    let _ = std::fs::remove_file(&tmp);
    Ok(content)
}

fn read_multiline_input() -> Result<String> {
    use crossterm::cursor;
    use crossterm::event::{self, Event, KeyCode, KeyModifiers};
    use crossterm::execute;
    use crossterm::terminal;

    let mut stdout = std::io::stdout();
    terminal::enable_raw_mode()?;
    struct RawGuard;
    impl Drop for RawGuard {
        fn drop(&mut self) {
            let _ = terminal::disable_raw_mode();
        }
    }
    let _guard = RawGuard;

    // Save position so redraw_input can restore to start of input
    execute!(stdout, cursor::SavePosition)?;

    let mut buffer = String::new();
    let result: Result<String> = loop {
        match event::read() {
            Ok(Event::Key(key)) => match (key.code, key.modifiers) {
                (KeyCode::Enter, mods) if mods == KeyModifiers::SHIFT => {
                    buffer.push('\n');
                    let _ = redraw_input(&mut stdout, &buffer);
                }
                (KeyCode::Enter, _) => break Ok(buffer),
                (KeyCode::Backspace, _) if !buffer.is_empty() => {
                    buffer.pop();
                    let _ = redraw_input(&mut stdout, &buffer);
                }
                (KeyCode::Char(c), mods)
                    if mods == KeyModifiers::CONTROL && (c == 'c' || c == 'd') =>
                {
                    bail!("Input cancelled.");
                }
                (KeyCode::Char(c), _) if !c.is_control() => {
                    buffer.push(c);
                    let _ = redraw_input(&mut stdout, &buffer);
                }
                _ => {}
            },
            Ok(_) => {}
            Err(e) => bail!("Terminal input error: {e}"),
        }
    };

    drop(_guard);
    println!();
    result
}

fn pager_load_batch_at(
    conn: &rusqlite::Connection,
    session_id: &str,
    show_tools: bool,
    offset: i64,
    count: i64,
    all_lines: &mut Vec<String>,
    insert_pos: usize,
) -> Result<usize> {
    let msgs = db::get_messages_with_parts_range(conn, session_id, offset, count)?;
    let before = all_lines.len();
    let markdown = render::render_message_batch(&msgs, show_tools);
    let rendered = render::render_markdown(&markdown);
    let mut batch: Vec<String> = rendered.lines().map(|l| l.to_string()).collect();
    // Splice at insert_pos (chronological order)
    let mut tail = all_lines.split_off(insert_pos);
    all_lines.append(&mut batch);
    all_lines.append(&mut tail);
    let added = all_lines.len() - before;
    Ok(added)
}

struct SessionLoader {
    sess_id: String,
    show_tools: bool,
    load_offset: i64,
}

impl pager::Loader for SessionLoader {
    fn load_more(&mut self) -> Vec<String> {
        if self.load_offset <= 0 {
            return vec![];
        }
        let conn = match db::open_db(None) {
            Ok(c) => c,
            Err(_) => {
                self.load_offset = 0;
                return vec![];
            }
        };
        let count = std::cmp::min(50i64, self.load_offset);
        let new_offset = self.load_offset - count;
        let msgs = match db::get_messages_with_parts_range(&conn, &self.sess_id, new_offset, count) {
            Ok(m) => m,
            Err(_) => {
                self.load_offset = 0;
                return vec![];
            }
        };
        let markdown = render::render_message_batch(&msgs, self.show_tools);
        let rendered = render::render_markdown(&markdown);
        let lines: Vec<String> = rendered.lines().map(|l| l.to_string()).collect();
        self.load_offset = new_offset;
        lines
    }
}

fn interactive_pager(
    conn: &rusqlite::Connection,
    session: &models::Session,
    show_tools: bool,
    note: Option<String>,
) -> Result<()> {
    let total_msgs = db::get_message_count(conn, &session.id)?;
    let mut all_lines: Vec<String> = Vec::new();
    let header_count: usize;

    {
        let header_str = render::render_session_header(session);
        let styled_header = render::render_markdown(&header_str);
        for line in styled_header.lines() {
            all_lines.push(line.to_string());
        }
        if let Some(ref n) = note {
            all_lines.push(String::new());
            all_lines.push(format!("\x1b[33m📝 Annotation: {}\x1b[0m", n));
            all_lines.push("\x1b[2m————————————————————————————————\x1b[0m".to_string());
            all_lines.push(String::new());
        }
        header_count = all_lines.len();
    }

    let mut load_offset = total_msgs;
    const BATCH_SIZE: i64 = 50;
    const INITIAL_BATCHES: i64 = 4;
    let mut batches_loaded = 0i64;
    while load_offset > 0 && batches_loaded < INITIAL_BATCHES {
        let count = std::cmp::min(BATCH_SIZE, load_offset);
        let new_offset = load_offset - count;
        if let Ok(_added) = pager_load_batch_at(
            conn, &session.id, show_tools, new_offset, count,
            &mut all_lines, header_count,
        ) {
            load_offset = new_offset;
            batches_loaded += 1;
        } else {
            break;
        }
    }

    let prefix = format!("{} | msgs:{}/{} | ", session.id, total_msgs, total_msgs);
    let mut p = pager::Pager::new(all_lines).with_status_prefix(&prefix);

    if load_offset > 0 {
        p.set_loader(header_count, Box::new(SessionLoader {
            sess_id: session.id.clone(),
            show_tools,
            load_offset,
        }));
    }

    p.run()?;
    Ok(())
}

fn run_text_pager(lines: &[String]) -> Result<()> {
    let owned: Vec<String> = lines.to_vec();
    let mut p = pager::Pager::new(owned);
    p.run()?;
    Ok(())
}

fn cmd_top(conn: &rusqlite::Connection, limit: i64, by: &str, json: bool) -> Result<()> {
    let valid_by = match by {
        "cost" | "tokens" | "msgs" => by,
        _ => bail!("Invalid sort: '{by}'. Use 'cost', 'tokens', or 'msgs'."),
    };
    let entries = db::top_sessions(conn, limit, valid_by)?;
    if entries.is_empty() {
        if json {
            println!("[]");
        } else {
            println!("No sessions found.");
        }
        return Ok(());
    }
    if json {
        let output = serde_json::to_string_pretty(&entries)?;
        println!("{output}");
    } else {
        let output = render::render_top_sessions(&entries, valid_by);
        println!("{output}");
    }
    Ok(())
}

fn cmd_watch(conn: &rusqlite::Connection, id: Option<&str>, poll_secs: u64, show_tools: bool, compact: bool) -> Result<()> {
    let sid = match id {
        Some(s) => s.to_string(),
        None => {
            let sessions = db::list_sessions(conn, 1, None, None, None, None, None, false, true)?;
            let s = sessions.first()
                .cloned()
                .with_context(|| "No sessions found.")?;
            eprintln!("Watching latest session: {} ({})", s.id, s.title);
            s.id
        }
    };

    validate_session_id(&sid)?;
    let session = db::get_session(conn, &sid)?
        .with_context(|| format!("Session not found: {}", sid))?;

    eprintln!("Watching session: {} ({})", sid, session.title);
    eprintln!("Polling every {}s. Press Ctrl+C to stop.\n", poll_secs);

    if compact {
        println!("{},{}", sid, session.title);
    }

    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = running.clone();
    if let Err(e) = ctrlc::set_handler(move || {
        r.store(false, std::sync::atomic::Ordering::SeqCst);
    }) {
        eprintln!("Warning: could not set Ctrl+C handler: {e}");
    }

    let mut last_count = db::get_message_count(conn, &sid)?;
    let mut last_data_version = db::get_data_version(conn)?;

    while running.load(std::sync::atomic::Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_secs(poll_secs));

        let dv = match db::get_data_version(conn) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if dv == last_data_version {
            continue;
        }
        last_data_version = dv;

        let count = match db::get_message_count(conn, &sid) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if count <= last_count {
            continue;
        }

        let messages = match db::get_messages(conn, &sid) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let msg_ids: Vec<String> = messages.iter().map(|m| m.id.clone()).collect();
        let parts_map = match db::get_parts_batch(conn, &msg_ids) {
            Ok(p) => p,
            Err(_) => continue,
        };

        let messages_with_parts: Vec<models::MessageWithParts> = messages
            .into_iter()
            .map(|msg| {
                let parts = parts_map
                    .iter()
                    .find(|(mid, _)| **mid == msg.id)
                    .map(|(_, parts)| parts.clone())
                    .unwrap_or_default();
                models::MessageWithParts { message: msg, parts }
            })
            .collect();

        for msg in messages_with_parts.get(last_count as usize..).unwrap_or(&[]) {
            if msg.message.data.role == "user" || msg.message.data.role == "assistant" {
                if compact {
                    let line = render::render_message_compact(msg);
                    println!("{}", line);
                } else {
                    let output = render::render_message_only(msg, show_tools);
                    println!("{}", output);
                }
            }
        }

        last_count = count;
    }

    eprintln!("\nStopped watching {}.", sid);
    Ok(())
}

fn cmd_tag(conn: &rusqlite::Connection, id: Option<&str>, tag: Option<&str>, remove: bool, list: bool, search: Option<&str>) -> Result<()> {
    if list {
        let tags = meta::all_tags()?;
        if tags.is_empty() {
            println!("No tags found.");
        } else {
            println!("{:<20} {}", "Tag", "Sessions");
            println!("{}", "-".repeat(30));
            for (t, count) in &tags {
                println!("{:<20} {}", t, count);
            }
        }
        return Ok(());
    }

    if let Some(query) = search {
        let results = meta::search_by_tag(&query)?;
        if results.is_empty() {
            println!("No sessions found with tag '{}'.", query);
        } else {
            let ids: Vec<String> = results.iter().map(|(id, _)| id.clone()).collect();
            let sessions = db::list_sessions_by_ids(conn, &ids)?;
            let output = render::render_session_list(&sessions);
            println!("{}", output);
        }
        return Ok(());
    }

    let sid = id.with_context(|| "Session ID is required (use `ocs tag <id> <tag>` or `--list` or `--search <tag>`)")?;
    validate_session_id(sid)?;

    if let Some(t) = tag {
        if remove {
            let found = meta::remove_tag(sid, t)?;
            if found {
                println!("Removed tag '{}' from session {}.", t, sid);
            } else {
                println!("Tag '{}' not found on session {}.", t, sid);
            }
        } else {
            meta::add_tag(sid, t)?;
            println!("Added tag '{}' to session {}.", t, sid);
        }
    } else {
        let tags = meta::list_tags(sid)?;
        if tags.is_empty() {
            println!("No tags for session {}.", sid);
        } else {
            println!("Tags for session {}: {}", sid, tags.join(", "));
        }
    }
    Ok(())
}

fn cmd_annotate(conn: &rusqlite::Connection, id: &str, text: Option<&str>, remove: bool) -> Result<()> {
    validate_session_id(id)?;

    if remove {
        let found = meta::remove_note(id)?;
        if found {
            println!("Removed annotation from session {}.", id);
        } else {
            println!("No annotation found for session {}.", id);
        }
        return Ok(());
    }

    match text {
        Some(t) => {
            meta::set_note(id, t)?;
            println!("Annotation saved for session {}.", id);
        }
        None => {
            match meta::get_note(id)? {
                Some(note) => {
                    let session = db::get_session(conn, id)?
                        .with_context(|| format!("Session not found: {id}"))?;
                    println!("📝 Annotation for **{}**:\n", session.title);
                    println!("{}\n", note);
                }
                None => {
                    println!("No annotation for session {}.", id);
                }
            }
        }
    }
    Ok(())
}

fn cmd_report(conn: &rusqlite::Connection, since: Option<&str>, until: Option<&str>, _project: Option<&str>, format: &str, output: Option<&str>) -> Result<()> {
    let now = chrono::Utc::now();
    let default_since = (now - chrono::Duration::days(14)).format("%Y-%m-%d").to_string();
    let default_until = now.format("%Y-%m-%d").to_string();

    let since_str = since.unwrap_or(&default_since);
    let until_str = until.unwrap_or(&default_until);

    let since_ts = parse_date(since_str)
        .with_context(|| format!("Invalid date: '{since_str}'. Use YYYY-MM-DD format."))?;
    let until_ts = parse_date(until_str)
        .with_context(|| format!("Invalid date: '{until_str}'. Use YYYY-MM-DD format."))?
        + 86_400_000; // include the full end day

    let summary = db::get_report_summary(conn, since_ts, until_ts)?;
    let trends = db::get_daily_trends(conn, since_ts, until_ts)?;
    let models = db::get_model_breakdown(conn, since_ts, until_ts)?;
    let sessions = db::top_sessions_in_range(conn, 10, since_ts, until_ts)?;

    let report = models::ReportSummary { period_start: since_str.to_string(), period_end: until_str.to_string(), ..summary };

    let content = match format {
        "markdown" => render::render_report_markdown(&report, &trends, &models, &sessions),
        "json" => render::render_report_json(&report, &trends, &models, &sessions)?,
        _ => bail!("Unsupported format: '{format}'. Use 'markdown' or 'json'."),
    };

    match output {
        Some(path) => safe_write(path, &content)?,
        None => println!("{content}"),
    }
    Ok(())
}

fn cmd_compare(conn: &rusqlite::Connection, id1: &str, id2: &str, _stats_only: bool, json: bool, output: Option<&str>) -> Result<()> {
    validate_session_id(id1)?;
    validate_session_id(id2)?;
    let s1 = db::get_session(conn, id1)?
        .with_context(|| format!("Session not found: {id1}"))?;
    let s2 = db::get_session(conn, id2)?
        .with_context(|| format!("Session not found: {id2}"))?;

    let m1 = db::get_messages(conn, id1)?;
    let m2 = db::get_messages(conn, id2)?;

    let ids1: Vec<String> = m1.iter().map(|m| m.id.clone()).collect();
    let ids2: Vec<String> = m2.iter().map(|m| m.id.clone()).collect();
    let p1 = db::get_parts_batch(conn, &ids1)?;
    let p2 = db::get_parts_batch(conn, &ids2)?;

    let attach_parts = |messages: Vec<models::Message>, parts_map: &std::collections::HashMap<String, Vec<models::Part>>| -> Vec<models::MessageWithParts> {
        messages.into_iter().map(|msg| {
            let parts = parts_map.iter()
                .find(|(mid, _)| **mid == msg.id)
                .map(|(_, p)| p.clone())
                .unwrap_or_default();
            models::MessageWithParts { message: msg, parts }
        }).collect()
    };

    let mp1 = attach_parts(m1, &p1);
    let mp2 = attach_parts(m2, &p2);

    let content = if json {
        render::render_compare_json(&s1, &s2, &mp1, &mp2)?
    } else {
        render::render_compare_markdown(&s1, &s2, &mp1, &mp2)?
    };

    match output {
        Some(path) => safe_write(path, &content)?,
        None => println!("{content}"),
    }
    Ok(())
}

fn cmd_export(conn: &rusqlite::Connection, id: &str, format: &str, output: Option<&str>, show_tools: bool, sanitize: bool) -> Result<()> {
    validate_session_id(id)?;
    let session = db::get_session(conn, id)?
        .with_context(|| format!("Session not found: {id}"))?;
    let messages = db::get_messages(conn, id)?;
    let msg_ids: Vec<String> = messages.iter().map(|m| m.id.clone()).collect();
    let parts_map = db::get_parts_batch(conn, &msg_ids)?;
    let messages_with_parts: Vec<models::MessageWithParts> = messages
        .into_iter()
        .map(|msg| {
            let parts = parts_map
                .iter()
                .find(|(mid, _)| **mid == msg.id)
                .map(|(_, parts)| parts.clone())
                .unwrap_or_default();
            models::MessageWithParts { message: msg, parts }
        })
        .collect();

    let content = match format {
        "markdown" => render::render_export_markdown(&session, &messages_with_parts, show_tools),
        "json" => render::render_export_json(&session, &messages_with_parts, sanitize)?,
        _ => bail!("Unsupported format: '{format}'. Use 'markdown' or 'json'."),
    };

    match output {
        Some(path) => safe_write(path, &content)?,
        None => println!("{content}"),
    }
    Ok(())
}

fn cmd_completion(shell: &str) -> Result<()> {
    use clap::CommandFactory;
    let shell: clap_complete::Shell = shell.parse()
        .map_err(|_| anyhow::anyhow!("Invalid shell: '{shell}'. Use bash, zsh, fish, powershell, or elvish."))?;
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    generate(shell, &mut cmd, name, &mut std::io::stdout());
    Ok(())
}

fn cmd_undo(conn: &rusqlite::Connection, session: Option<&str>, cont: bool, project: Option<&str>) -> Result<()> {
    let sid = match session {
        Some(s) => {
            validate_session_id(s)?;
            s.to_string()
        }
        None if cont => {
            let sessions = db::list_sessions(conn, 1, None, None, None, project, None, false, true)?;
            sessions.first()
                .cloned()
                .with_context(|| "No sessions found.")?
                .id
        }
        None => {
            let sessions = db::list_sessions(conn, 50, None, None, None, project, None, false, true)?;
            if sessions.is_empty() {
                bail!("No sessions found.");
            }
            pick_session_interactive(conn, &sessions)?
        }
    };

    db::get_session(conn, &sid)?
        .with_context(|| format!("Session not found: {sid}"))?;

    let messages = db::get_messages(conn, &sid)?;
    let user_messages: Vec<&models::Message> = messages.iter()
        .filter(|m| m.data.role == "user")
        .collect();

    if user_messages.is_empty() {
        bail!("No user messages found in session {sid}.");
    }

    let msg_ids: Vec<String> = user_messages.iter().map(|m| m.id.clone()).collect();
    let parts_map = db::get_parts_batch(conn, &msg_ids)?;

    let selected_id = pick_message_interactive(&user_messages, &parts_map)?;

    print!("[d]elete  [e]dit  [q]uit: ");
    std::io::stdout().flush()?;
    let mut action = String::new();
    std::io::stdin().read_line(&mut action)?;
    match action.trim().to_lowercase().as_str() {
        "d" => {
            let rw_conn = db::open_db_rw(None)?;
            if db::delete_message(&rw_conn, &selected_id)? {
                println!("Message {} deleted from {}.", selected_id, sid);
            } else {
                eprintln!("Failed to delete message {}.", selected_id);
            }
        }
        "e" => {
            let text = db::get_message_text(conn, &selected_id)?;
            let new_text = edit_message_text(&text)?;
            if new_text == text {
                println!("No changes made.");
            } else {
                let rw_conn = db::open_db_rw(None)?;
                db::update_message_text(&rw_conn, &selected_id, &new_text)?;
                println!("Message {} updated.", selected_id);
            }
        }
        _ => {
            println!("Cancelled.");
        }
    }

    Ok(())
}

fn pick_message_interactive<'a>(
    messages: &[&models::Message],
    parts_map: &std::collections::HashMap<String, Vec<models::Part>>,
) -> Result<String> {
    let lines: Vec<String> = messages.iter().map(|m| {
        let ts = format_timestamp(m.time_created);
        let preview = parts_map.get(&m.id)
            .and_then(|parts| {
                parts.iter()
                    .find(|p| p.data.r#type == "text" || p.data.r#type == "reasoning")
                    .and_then(|p| p.data.text.as_deref())
            })
            .unwrap_or("")
            .chars()
            .take(80)
            .collect::<String>();
        format!("{}\t{}  {}\t{}", m.id, ts, m.data.role, preview)
    }).collect();

    let selector = if which("peco").is_ok() {
        "peco"
    } else if which("fzf").is_ok() {
        "fzf"
    } else {
        bail!("Neither peco nor fzf found. Install one for interactive selection.");
    };

    let mut child = std::process::Command::new(selector)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .with_context(|| format!("Failed to spawn {selector}"))?;

    if let Some(stdin) = child.stdin.as_mut() {
        for line in &lines {
            writeln!(stdin, "{line}")?;
        }
    }

    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!("Selection cancelled.");
    }

    let selected = String::from_utf8(output.stdout)
        .with_context(|| "peco/fzf output contained invalid UTF-8")?;
    selected.lines().next()
        .and_then(|line| line.split('\t').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .with_context(|| "No message selected.")
}

fn edit_message_text(current_text: &str) -> Result<String> {
    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "vi".to_string());
    let tmp = format!("/tmp/ocs_message_edit_{}.md", std::process::id());
    std::fs::write(&tmp, current_text)
        .with_context(|| "Failed to write temp file for editor")?;
    let status = std::process::Command::new(&editor)
        .arg(&tmp)
        .status()
        .with_context(|| format!("Failed to launch editor '{editor}'"))?;
    if !status.success() {
        let _ = std::fs::remove_file(&tmp);
        bail!("Editor exited unsuccessfully.");
    }
    let content = std::fs::read_to_string(&tmp)
        .with_context(|| "Failed to read editor output")?;
    let _ = std::fs::remove_file(&tmp);
    Ok(content)
}

fn format_timestamp(ts_ms: i64) -> String {
    let secs = ts_ms / 1000;
    let nsecs = ((ts_ms % 1000) * 1_000_000) as u32;
    match chrono::DateTime::from_timestamp(secs, nsecs) {
        Some(dt) => dt.format("%H:%M:%S").to_string(),
        None => "??:??:??".to_string(),
    }
}

/// Atomic file write: creates parent dirs, writes to .tmp, then renames
///
/// Uses O_EXCL to prevent symlink races (TOCTOU) — if a symlink already
/// exists at the temp path, the write fails instead of following it.
fn safe_write(path: &str, content: &str) -> Result<()> {
    if path.contains("..") {
        bail!("Output path must not contain '..': {path}");
    }
    let p = std::path::Path::new(path);
    if let Some(parent) = p.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory for {path}"))?;
        }
    }
    let tmp_path = format!("{path}.tmp");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .open(&tmp_path)
        .with_context(|| format!("Failed to create temporary file {tmp_path}"))?;
    file.write_all(content.as_bytes())
        .with_context(|| format!("Failed to write to {tmp_path}"))?;
    file.flush()?;
    std::fs::rename(&tmp_path, path)
        .with_context(|| format!("Failed to rename {tmp_path} to {path}"))?;
    Ok(())
}

fn parse_date(s: &str) -> Option<i64> {
    // Try YYYY-MM-DD HH:MM:SS
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Some(dt.and_utc().timestamp_millis());
    }
    // Try YYYY-MM-DD (start of day)
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        if let Some(dt) = d.and_hms_opt(0, 0, 0) {
            return Some(dt.and_utc().timestamp_millis());
        }
    }
    eprintln!("Warning: could not parse date '{s}'. Expected YYYY-MM-DD or YYYY-MM-DD HH:MM:SS");
    None
}

fn which(name: &str) -> std::io::Result<std::process::Output> {
    Command::new("which").arg(name).output()
}
