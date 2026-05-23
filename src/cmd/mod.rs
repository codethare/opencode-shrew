mod list;
mod show;
mod run;
mod manage;
mod analytics;
mod completion;

pub use list::cmd_list;
pub use show::{cmd_show, cmd_search, cmd_diff};
pub use run::{cmd_run, cmd_watch};
pub use manage::{cmd_rename, cmd_prune, cmd_tag, cmd_annotate, cmd_undo, cmd_export};
pub use analytics::{cmd_stats, cmd_top, cmd_projects, cmd_report, cmd_compare, cmd_dashboard};
pub use completion::cmd_completion;

use anyhow::{bail, Context, Result};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;

use crate::render;

/// Set to true when we intentionally exit after subprocess completion
pub static SUBPROCESS_EXIT: AtomicBool = AtomicBool::new(false);

pub fn validate_session_id(id: &str) -> Result<()> {
    if id.contains("..") || id.contains('/') || id.contains('\0') {
        bail!("Invalid session ID: path traversal detected");
    }
    Ok(())
}

pub fn format_timestamp(ts_ms: i64) -> String {
    let secs = ts_ms / 1000;
    let nsecs = ((ts_ms % 1000) * 1_000_000) as u32;
    match chrono::DateTime::from_timestamp(secs, nsecs) {
        Some(dt) => dt.format("%H:%M:%S").to_string(),
        None => "??:??:??".to_string(),
    }
}

/// Pick a session interactively via peco/fzf, returns the selected session ID
pub fn pick_session_interactive(
    _conn: &rusqlite::Connection,
    sessions: &[crate::models::Session],
) -> Result<String> {
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
    selected
        .lines()
        .next()
        .and_then(|line| line.split('\t').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .with_context(|| "No session selected.")
}

fn which(name: &str) -> std::io::Result<std::process::Output> {
    Command::new("which").arg(name).output()
}

/// Atomic file write: creates parent dirs, writes to .tmp, then renames
///
/// Uses O_EXCL to prevent symlink races (TOCTOU) — if a symlink already
/// exists at the temp path, the write fails instead of following it.
pub fn safe_write(path: &str, content: &str) -> Result<()> {
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

pub fn parse_date(s: &str) -> Option<i64> {
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
