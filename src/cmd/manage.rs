use anyhow::{bail, Context, Result};
use std::io::Write;

use crate::db;
use crate::meta;
use crate::models;
use crate::render;

pub fn cmd_rename(_conn: &rusqlite::Connection, id: &str, title: &str) -> Result<()> {
    super::validate_session_id(id)?;
    let rw_conn = db::open_db_rw(None)?;
    if db::rename_session(&rw_conn, id, title)? {
        println!("Session {id} renamed to: {title}");
    } else {
        eprintln!("Session not found: {id}");
    }
    Ok(())
}

pub fn cmd_prune(conn: &rusqlite::Connection, days: i64, dry_run: bool, force: bool) -> Result<()> {
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
    println!("Deleted {deleted} session(s).",);
    if failed > 0 {
        eprintln!("{failed} session(s) could not be deleted.");
    }
    Ok(())
}

pub fn cmd_tag(
    conn: &rusqlite::Connection,
    id: Option<&str>,
    tag: Option<&str>,
    remove: bool,
    list: bool,
    search: Option<&str>,
) -> Result<()> {
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

    let sid = id.with_context(|| {
        "Session ID is required (use `ocs tag <id> <tag>` or `--list` or `--search <tag>`)"
    })?;
    super::validate_session_id(sid)?;

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

pub fn cmd_annotate(
    conn: &rusqlite::Connection,
    id: &str,
    text: Option<&str>,
    remove: bool,
) -> Result<()> {
    super::validate_session_id(id)?;

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
        None => match meta::get_note(id)? {
            Some(note) => {
                let session = db::get_session(conn, id)?
                    .with_context(|| format!("Session not found: {id}"))?;
                println!("📝 Annotation for **{}**:\n", session.title);
                println!("{}\n", note);
            }
            None => {
                println!("No annotation for session {}.", id);
            }
        },
    }
    Ok(())
}

pub fn cmd_undo(
    conn: &rusqlite::Connection,
    session: Option<&str>,
    cont: bool,
    project: Option<&str>,
) -> Result<()> {
    let sid = match session {
        Some(s) => {
            super::validate_session_id(s)?;
            s.to_string()
        }
        None if cont => {
            let sessions =
                db::list_sessions(conn, 1, None, None, None, project, None, false, true)?;
            sessions
                .first()
                .cloned()
                .with_context(|| "No sessions found.")?
                .id
        }
        None => {
            let sessions =
                db::list_sessions(conn, 50, None, None, None, project, None, false, true)?;
            if sessions.is_empty() {
                bail!("No sessions found.");
            }
            super::pick_session_interactive(conn, &sessions)?
        }
    };

    db::get_session(conn, &sid)?
        .with_context(|| format!("Session not found: {sid}"))?;

    let messages = db::get_messages(conn, &sid)?;
    let user_messages: Vec<&models::Message> =
        messages.iter().filter(|m| m.data.role == "user").collect();

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

pub fn cmd_export(
    conn: &rusqlite::Connection,
    id: &str,
    format: &str,
    output: Option<&str>,
    show_tools: bool,
    sanitize: bool,
) -> Result<()> {
    super::validate_session_id(id)?;
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
        Some(path) => super::safe_write(path, &content)?,
        None => println!("{content}"),
    }
    Ok(())
}

fn pick_message_interactive<'a>(
    messages: &[&models::Message],
    parts_map: &std::collections::HashMap<String, Vec<models::Part>>,
) -> Result<String> {
    let lines: Vec<String> = messages
        .iter()
        .map(|m| {
            let ts = super::format_timestamp(m.time_created);
            let preview = parts_map
                .get(&m.id)
                .and_then(|parts| {
                    parts
                        .iter()
                        .find(|p| p.data.r#type == "text" || p.data.r#type == "reasoning")
                        .and_then(|p| p.data.text.as_deref())
                })
                .unwrap_or("")
                .chars()
                .take(80)
                .collect::<String>();
            format!("{}\t{}  {}\t{}", m.id, ts, m.data.role, preview)
        })
        .collect();

    let selector = if super::which("peco").is_ok() {
        "peco"
    } else if super::which("fzf").is_ok() {
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
    selected
        .lines()
        .next()
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
