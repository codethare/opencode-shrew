use anyhow::{bail, Context, Result};
use crate::db;
use crate::models;
use crate::render;

pub fn cmd_stats(conn: &rusqlite::Connection, id: &str, json: bool) -> Result<()> {
    super::validate_session_id(id)?;
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

pub fn cmd_top(conn: &rusqlite::Connection, limit: i64, by: &str, json: bool) -> Result<()> {
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

pub fn cmd_projects(conn: &rusqlite::Connection, limit: i64, json: bool) -> Result<()> {
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

pub fn cmd_report(
    conn: &rusqlite::Connection,
    since: Option<&str>,
    until: Option<&str>,
    _project: Option<&str>,
    format: &str,
    output: Option<&str>,
) -> Result<()> {
    let now = chrono::Utc::now();
    let default_since = (now - chrono::Duration::days(14))
        .format("%Y-%m-%d")
        .to_string();
    let default_until = now.format("%Y-%m-%d").to_string();

    let since_str = since.unwrap_or(&default_since);
    let until_str = until.unwrap_or(&default_until);

    let since_ts = super::parse_date(since_str)
        .with_context(|| format!("Invalid date: '{since_str}'. Use YYYY-MM-DD format."))?;
    let until_ts = super::parse_date(until_str)
        .with_context(|| format!("Invalid date: '{until_str}'. Use YYYY-MM-DD format."))?
        + 86_400_000; // include the full end day

    let summary = db::get_report_summary(conn, since_ts, until_ts)?;
    let trends = db::get_daily_trends(conn, since_ts, until_ts)?;
    let models = db::get_model_breakdown(conn, since_ts, until_ts)?;
    let sessions = db::top_sessions_in_range(conn, 10, since_ts, until_ts)?;

    let report = models::ReportSummary {
        period_start: since_str.to_string(),
        period_end: until_str.to_string(),
        ..summary
    };

    let content = match format {
        "markdown" => render::render_report_markdown(&report, &trends, &models, &sessions),
        "json" => render::render_report_json(&report, &trends, &models, &sessions)?,
        _ => bail!("Unsupported format: '{format}'. Use 'markdown' or 'json'."),
    };

    match output {
        Some(path) => super::safe_write(path, &content)?,
        None => println!("{content}"),
    }
    Ok(())
}

pub fn cmd_dashboard(conn: &rusqlite::Connection, limit: i64, json: bool) -> Result<()> {
    let dash = db::get_dashboard(conn, limit)?;
    if json {
        let output = serde_json::to_string_pretty(&dash)?;
        println!("{output}");
    } else {
        let output = render::render_dashboard(&dash);
        println!("{output}");
    }
    Ok(())
}

pub fn cmd_compare(
    conn: &rusqlite::Connection,
    id1: &str,
    id2: &str,
    _stats_only: bool,
    json: bool,
    output: Option<&str>,
) -> Result<()> {
    super::validate_session_id(id1)?;
    super::validate_session_id(id2)?;
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

    let attach_parts =
        |messages: Vec<models::Message>,
         parts_map: &std::collections::HashMap<String, Vec<models::Part>>|
         -> Vec<models::MessageWithParts> {
            messages
                .into_iter()
                .map(|msg| {
                    let parts = parts_map
                        .iter()
                        .find(|(mid, _)| **mid == msg.id)
                        .map(|(_, p)| p.clone())
                        .unwrap_or_default();
                    models::MessageWithParts { message: msg, parts }
                })
                .collect()
        };

    let mp1 = attach_parts(m1, &p1);
    let mp2 = attach_parts(m2, &p2);

    let content = if json {
        render::render_compare_json(&s1, &s2, &mp1, &mp2)?
    } else {
        render::render_compare_markdown(&s1, &s2, &mp1, &mp2)?
    };

    match output {
        Some(path) => super::safe_write(path, &content)?,
        None => println!("{content}"),
    }
    Ok(())
}
