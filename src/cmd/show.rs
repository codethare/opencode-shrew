use anyhow::{Context, Result};

use crate::db;
use crate::meta;
use crate::models;
use crate::pager;
use crate::render;

pub fn cmd_show(
    conn: &rusqlite::Connection,
    id: &str,
    raw: bool,
    show_tools: bool,
    sanitize: bool,
) -> Result<()> {
    super::validate_session_id(id)?;
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

pub fn cmd_search(
    conn: &rusqlite::Connection,
    query: &str,
    limit: i64,
    offset: Option<i64>,
    since: Option<i64>,
    until: Option<i64>,
    json: bool,
) -> Result<()> {
    let results = db::search_sessions(conn, query, limit, offset, since, until)?;
    if json {
        let output = serde_json::to_string_pretty(&results)?;
        println!("{output}");
    } else {
        let output = render::render_search_results(&results, query);
        println!("{output}");
    }
    Ok(())
}

pub fn cmd_diff(conn: &rusqlite::Connection, id: &str) -> Result<()> {
    super::validate_session_id(id)?;
    let _session = db::get_session(conn, id)?
        .with_context(|| format!("Session not found: {id}"))?;

    let entries = db::read_session_diff(id)?;
    let output = render::render_diff(&entries, id);
    println!("{output}");
    Ok(())
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
        p.set_loader(
            header_count,
            Box::new(SessionLoader {
                sess_id: session.id.clone(),
                show_tools,
                load_offset,
            }),
        );
    }

    p.run()?;
    Ok(())
}
