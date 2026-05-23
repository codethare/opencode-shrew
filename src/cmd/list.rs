use anyhow::Result;

use crate::db;
use crate::meta;
use crate::models;
use crate::render;

pub fn cmd_list(
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
    let since_ts = since.and_then(super::parse_date);
    let until_ts = until.and_then(super::parse_date);

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

fn cmd_list_interactive(conn: &rusqlite::Connection, sessions: &[models::Session]) -> Result<()> {
    match super::pick_session_interactive(conn, sessions) {
        Ok(id) => super::cmd_show(conn, &id, false, true, false),
        Err(e) => {
            eprintln!("{e}");
            Ok(())
        }
    }
}
