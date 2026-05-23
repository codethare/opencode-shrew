use anyhow::{bail, Context, Result};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::Ordering;

use crate::db;
use crate::models;
use crate::pager;
use crate::render;

pub fn cmd_run(
    conn: &rusqlite::Connection,
    message: Option<&str>,
    session: Option<&str>,
    fork: bool,
    interactive: bool,
    input: bool,
    cont: bool,
    edit: bool,
    project: Option<&str>,
) -> Result<()> {
    if super::which("opencode").is_err() {
        bail!("'opencode' binary not found in PATH.");
    }

    let sid = match session {
        Some(s) => {
            super::validate_session_id(s)?;
            s.to_string()
        }
        None if interactive => {
            let sessions = db::list_sessions(conn, 50, None, None, None, project, None, false, true)?;
            if sessions.is_empty() {
                bail!("No sessions found.");
            }
            super::pick_session_interactive(conn, &sessions)?
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
            super::SUBPROCESS_EXIT.store(true, Ordering::Relaxed);
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

        super::SUBPROCESS_EXIT.store(true, Ordering::Relaxed);
        std::process::exit(status.code().unwrap_or(1));
    }
}

pub fn cmd_watch(
    conn: &rusqlite::Connection,
    id: Option<&str>,
    poll_secs: u64,
    show_tools: bool,
    compact: bool,
) -> Result<()> {
    let sid = match id {
        Some(s) => s.to_string(),
        None => {
            let sessions = db::list_sessions(conn, 1, None, None, None, None, None, false, true)?;
            let s = sessions
                .first()
                .cloned()
                .with_context(|| "No sessions found.")?;
            eprintln!("Watching latest session: {} ({})", s.id, s.title);
            s.id
        }
    };

    super::validate_session_id(&sid)?;
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

fn redraw_input(stdout: &mut std::io::Stdout, buffer: &str) -> std::io::Result<()> {
    use crossterm::cursor;
    use crossterm::execute;
    use crossterm::terminal::{Clear, ClearType};
    execute!(
        stdout,
        cursor::RestorePosition,
        Clear(ClearType::FromCursorDown)
    )?;
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

fn run_text_pager(lines: &[String]) -> Result<()> {
    let owned: Vec<String> = lines.to_vec();
    let mut p = pager::Pager::new(owned);
    p.run()?;
    Ok(())
}
