use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use rusqlite::Connection;

use crate::models::*;

pub fn default_db_path() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/oc".to_string());
    format!("{}/.local/share/opencode/opencode.db", home)
}

pub fn open_db(path: Option<&str>) -> Result<Connection> {
    let db_path = path.unwrap_or_else(|| {
        Box::leak(default_db_path().into_boxed_str())
    });
    let conn = Connection::open(db_path)
        .with_context(|| format!("Failed to open database at {db_path}"))?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA query_only=ON;")?;
    Ok(conn)
}

pub fn open_db_rw(path: Option<&str>) -> Result<Connection> {
    let db_path = path.unwrap_or_else(|| {
        Box::leak(default_db_path().into_boxed_str())
    });
    let conn = Connection::open(db_path)
        .with_context(|| format!("Failed to open database at {db_path}"))?;
    Ok(conn)
}

pub fn list_sessions(
    conn: &Connection,
    limit: i64,
    search: Option<&str>,
    since: Option<i64>,
    until: Option<i64>,
    project: Option<&str>,
    min_msgs: Option<i64>,
    cli_only: bool,
    show_all: bool,
) -> Result<Vec<Session>> {
    let mut sql = String::from(
        "SELECT s.id, s.project_id, s.slug, s.directory, s.title, \
                s.time_created, s.time_updated, \
                s.summary_additions, s.summary_deletions, s.summary_files, \
                s.parent_id, \
                (SELECT COUNT(*) FROM message m WHERE m.session_id = s.id) AS msg_count, \
                (SELECT json_extract(m.data, '$.model.modelID') FROM message m \
                 WHERE m.session_id = s.id AND m.data LIKE '%model%' LIMIT 1) AS model, \
                COALESCE((SELECT SUM(json_extract(m.data, '$.cost')) FROM message m \
                 WHERE m.session_id = s.id AND json_extract(m.data, '$.cost') IS NOT NULL), 0.0) AS total_cost \
         FROM session s"
    );

    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut param_idx = 0;

    if let Some(term) = search {
        param_idx += 1;
        conditions.push(format!("(s.title LIKE ?{param_idx} ESCAPE '\\' OR s.id LIKE ?{param_idx} ESCAPE '\\')"));
        params.push(Box::new(format!("%{}%", term.replace('%', "\\%").replace('_', "\\_"))));
    }
    if let Some(ts) = since {
        param_idx += 1;
        conditions.push(format!("s.time_created >= ?{param_idx}"));
        params.push(Box::new(ts));
    }
    if let Some(ts) = until {
        param_idx += 1;
        conditions.push(format!("s.time_created <= ?{param_idx}"));
        params.push(Box::new(ts));
    }
    if let Some(dir) = project {
        let escaped = dir.replace('%', "\\%").replace('_', "\\_");
        let like_pattern = format!("%/{}", escaped);
        param_idx += 1;
        let exact_idx = param_idx;
        param_idx += 1;
        let like_idx = param_idx;
        conditions.push(format!(
            "(s.directory = ?{exact_idx} OR s.directory LIKE ?{like_idx} ESCAPE '\\')"
        ));
        params.push(Box::new(dir.to_string()));
        params.push(Box::new(like_pattern));
    }

    if let Some(min) = min_msgs {
        param_idx += 1;
        conditions.push(format!(
            "(SELECT COUNT(*) FROM message m WHERE m.session_id = s.id) >= ?{param_idx}"
        ));
        params.push(Box::new(min));
    }

    if !show_all {
        conditions.push("s.parent_id IS NULL".to_string());
    }

    if cli_only {
        conditions.push("s.permission IS NULL".to_string());
    }

    if !conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));
    }

    sql.push_str(" ORDER BY s.time_created DESC");

    if limit > 0 {
        param_idx += 1;
        sql.push_str(&format!(" LIMIT ?{param_idx}"));
        params.push(Box::new(limit));
    }

    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        Ok(Session {
            id: row.get("id")?,
            project_id: row.get("project_id")?,
            slug: row.get("slug")?,
            directory: row.get("directory")?,
            title: row.get("title")?,
            time_created: row.get("time_created")?,
            time_updated: row.get("time_updated")?,
            summary_additions: row.get("summary_additions")?,
            summary_deletions: row.get("summary_deletions")?,
            summary_files: row.get("summary_files")?,
            msg_count: row.get("msg_count")?,
            model: row.get("model")?,
            total_cost: row.get("total_cost")?,
            parent_id: row.get("parent_id")?,
        })
    })?;

    let mut sessions = Vec::new();
    for row in rows {
        sessions.push(row?);
    }
    Ok(sessions)
}

pub fn get_data_version(conn: &Connection) -> Result<i64> {
    let val: i64 = conn.pragma_query_value(None, "data_version", |row| row.get(0))?;
    Ok(val)
}

pub fn list_sessions_by_ids(conn: &Connection, ids: &[String]) -> Result<Vec<Session>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders: Vec<String> = ids.iter().enumerate()
        .map(|(i, _)| format!("?{}", i + 1))
        .collect();
    let sql = format!(
        "SELECT s.id, s.project_id, s.slug, s.directory, s.title, \
                s.time_created, s.time_updated, \
                s.summary_additions, s.summary_deletions, s.summary_files, \
                (SELECT COUNT(*) FROM message m WHERE m.session_id = s.id) AS msg_count, \
                (SELECT json_extract(m.data, '$.model.modelID') FROM message m \
                 WHERE m.session_id = s.id AND m.data LIKE '%model%' LIMIT 1) AS model, \
                COALESCE((SELECT SUM(json_extract(m.data, '$.cost')) FROM message m \
                 WHERE m.session_id = s.id AND json_extract(m.data, '$.cost') IS NOT NULL), 0.0) AS total_cost, \
                 s.parent_id \
         FROM session s WHERE s.id IN ({}) \
         ORDER BY s.time_created DESC",
        placeholders.join(",")
    );
    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = ids
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();
    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        Ok(Session {
            id: row.get("id")?,
            project_id: row.get("project_id")?,
            slug: row.get("slug")?,
            directory: row.get("directory")?,
            title: row.get("title")?,
            time_created: row.get("time_created")?,
            time_updated: row.get("time_updated")?,
            summary_additions: row.get("summary_additions")?,
            summary_deletions: row.get("summary_deletions")?,
            summary_files: row.get("summary_files")?,
            msg_count: row.get("msg_count")?,
            model: row.get("model")?,
            total_cost: row.get("total_cost")?,
            parent_id: row.get("parent_id")?,
        })
    })?;
    let mut sessions = Vec::new();
    for row in rows {
        sessions.push(row?);
    }
    Ok(sessions)
}

pub fn get_session(conn: &Connection, id: &str) -> Result<Option<Session>> {
    let sql = "SELECT s.id, s.project_id, s.slug, s.directory, s.title, \
               s.time_created, s.time_updated, \
               s.summary_additions, s.summary_deletions, s.summary_files, \
               s.parent_id, \
               (SELECT COUNT(*) FROM message m WHERE m.session_id = s.id) AS msg_count, \
               (SELECT json_extract(m.data, '$.model.modelID') FROM message m \
                WHERE m.session_id = s.id AND m.data LIKE '%model%' LIMIT 1) AS model, \
               COALESCE((SELECT SUM(json_extract(m.data, '$.cost')) FROM message m \
                WHERE m.session_id = s.id AND json_extract(m.data, '$.cost') IS NOT NULL), 0.0) AS total_cost \
         FROM session s WHERE s.id = ?1";
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query_map([id], |row| {
        Ok(Session {
            id: row.get("id")?,
            project_id: row.get("project_id")?,
            slug: row.get("slug")?,
            directory: row.get("directory")?,
            title: row.get("title")?,
            time_created: row.get("time_created")?,
            time_updated: row.get("time_updated")?,
            summary_additions: row.get("summary_additions")?,
            summary_deletions: row.get("summary_deletions")?,
            summary_files: row.get("summary_files")?,
            msg_count: row.get("msg_count")?,
            model: row.get("model")?,
            total_cost: row.get("total_cost")?,
            parent_id: row.get("parent_id")?,
        })
    })?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}


pub fn get_related_sessions(conn: &Connection, id: &str, directory: &str, limit: i64) -> Result<Vec<Session>> {
    let sql = "SELECT s.id, s.project_id, s.slug, s.directory, s.title, \
               s.time_created, s.time_updated, \
               s.summary_additions, s.summary_deletions, s.summary_files, \
               s.parent_id, \
               (SELECT COUNT(*) FROM message m WHERE m.session_id = s.id) AS msg_count, \
               (SELECT json_extract(m.data, '$.model.modelID') FROM message m \
                WHERE m.session_id = s.id AND m.data LIKE '%model%' LIMIT 1) AS model, \
               COALESCE((SELECT SUM(json_extract(m.data, '$.cost')) FROM message m \
                WHERE m.session_id = s.id AND json_extract(m.data, '$.cost') IS NOT NULL), 0.0) AS total_cost \
         FROM session s WHERE s.directory = ?1 AND s.id != ?2 \
         ORDER BY s.time_created DESC LIMIT ?3";
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(rusqlite::params![directory, id, limit], |row| {
        Ok(Session {
            id: row.get("id")?,
            project_id: row.get("project_id")?,
            slug: row.get("slug")?,
            directory: row.get("directory")?,
            title: row.get("title")?,
            time_created: row.get("time_created")?,
            time_updated: row.get("time_updated")?,
            summary_additions: row.get("summary_additions")?,
            summary_deletions: row.get("summary_deletions")?,
            summary_files: row.get("summary_files")?,
            msg_count: row.get("msg_count")?,
            model: row.get("model")?,
            total_cost: row.get("total_cost")?,
            parent_id: row.get("parent_id")?,
        })
    })?;
    let mut sessions = Vec::new();
    for row in rows {
        sessions.push(row?);
    }
    Ok(sessions)
}

pub fn get_messages(conn: &Connection, session_id: &str) -> Result<Vec<Message>> {
    let sql = "SELECT id, session_id, time_created, data \
               FROM message WHERE session_id = ?1 \
               ORDER BY time_created ASC, id ASC";
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([session_id], |row| {
        let data_str: String = row.get("data")?;
        let data: MessageData = serde_json::from_str(&data_str)
            .unwrap_or_else(|e| {
                eprintln!("Warning: failed to parse message data JSON: {e}");
                MessageData {
                    role: "unknown".to_string(),
                    agent: None,
                    model: None,
                    tokens: None,
                    cost: None,
                    mode: None,
                    parent_id: None,
                }
            });
        Ok(Message {
            id: row.get("id")?,
            session_id: row.get("session_id")?,
            time_created: row.get("time_created")?,
            data,
        })
    })?;
    let mut messages = Vec::new();
    for row in rows {
        messages.push(row?);
    }
    Ok(messages)
}

pub fn get_parts_batch(conn: &Connection, message_ids: &[String]) -> Result<std::collections::HashMap<String, Vec<Part>>> {
    if message_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders: Vec<String> = message_ids.iter().enumerate()
        .map(|(i, _)| format!("?{}", i + 1))
        .collect();
    let sql = format!(
        "SELECT id, message_id, data FROM part WHERE message_id IN ({}) ORDER BY id ASC",
        placeholders.join(",")
    );
    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = message_ids
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();
    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        let data_str: String = row.get("data")?;
        let data: PartData = serde_json::from_str(&data_str)
            .unwrap_or_else(|e| {
                eprintln!("Warning: failed to parse part data JSON: {e}");
                PartData {
                    r#type: "unknown".to_string(),
                    text: None,
                    tool: None,
                    call_id: None,
                    state: None,
                    reason: None,
                    time: None,
                    done: None,
                    body: None,
                }
            });
        Ok(Part {
            id: row.get("id")?,
            message_id: row.get("message_id")?,
            data,
        })
    })?;
    let mut parts_by_msg: std::collections::HashMap<String, Vec<Part>> =
        message_ids.iter().map(|id| (id.clone(), Vec::new())).collect();
    for row in rows {
        let part = row?;
        if let Some(vec) = parts_by_msg.get_mut(&part.message_id) {
            vec.push(part);
        }
    }
    Ok(parts_by_msg)
}

pub fn get_messages_with_parts(conn: &Connection, session_id: &str) -> Result<Vec<MessageWithParts>> {
    let messages = get_messages(conn, session_id)?;
    let msg_ids: Vec<String> = messages.iter().map(|m| m.id.clone()).collect();
    let mut parts_map = get_parts_batch(conn, &msg_ids)?;
    Ok(messages
        .into_iter()
        .map(|msg| {
            let msg_id = msg.id.clone();
            MessageWithParts {
                message: msg,
                parts: parts_map.remove(&msg_id).unwrap_or_default(),
            }
        })
        .collect())
}

pub fn get_message_count(conn: &Connection, session_id: &str) -> Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM message WHERE session_id = ?1",
        [session_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn get_messages_range(
    conn: &Connection,
    session_id: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<Message>> {
    let sql = "SELECT id, session_id, time_created, data \
               FROM message WHERE session_id = ?1 \
               ORDER BY time_created ASC, id ASC \
               LIMIT ?2 OFFSET ?3";
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(rusqlite::params![session_id, limit, offset], |row| {
        let data_str: String = row.get("data")?;
        let data: MessageData = serde_json::from_str(&data_str).unwrap_or_else(|e| {
            eprintln!("Warning: failed to parse message data JSON: {e}");
            MessageData {
                role: "unknown".to_string(),
                agent: None,
                model: None,
                tokens: None,
                cost: None,
                mode: None,
                parent_id: None,
            }
        });
        Ok(Message {
            id: row.get("id")?,
            session_id: row.get("session_id")?,
            time_created: row.get("time_created")?,
            data,
        })
    })?;
    let mut messages = Vec::new();
    for row in rows {
        messages.push(row?);
    }
    Ok(messages)
}

pub fn get_messages_with_parts_range(
    conn: &Connection,
    session_id: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<MessageWithParts>> {
    let messages = get_messages_range(conn, session_id, offset, limit)?;
    let msg_ids: Vec<String> = messages.iter().map(|m| m.id.clone()).collect();
    let mut parts_map = get_parts_batch(conn, &msg_ids)?;
    Ok(messages
        .into_iter()
        .map(|msg| {
            let msg_id = msg.id.clone();
            MessageWithParts {
                message: msg,
                parts: parts_map.remove(&msg_id).unwrap_or_default(),
            }
        })
        .collect())
}

/// Search session messages content for a query string
pub fn search_sessions(conn: &Connection, query: &str, limit: i64) -> Result<Vec<SearchResult>> {
    let sql = "\
        SELECT s.id AS session_id, s.title, s.time_created, \
               m.id AS message_id, m.time_created AS msg_time_created, \
               p.id AS part_id, p.data AS part_data \
        FROM part p \
        JOIN message m ON m.id = p.message_id \
        JOIN session s ON s.id = m.session_id \
        WHERE p.data LIKE ?1 ESCAPE '\\' AND json_extract(p.data, '$.type') IN ('text', 'reasoning') \
        ORDER BY m.time_created DESC \
        LIMIT ?2";

    let pattern = format!("%{}%", query.replace('%', "\\%").replace('_', "\\_"));
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(rusqlite::params![pattern, limit], |row| {
        let part_data_str: String = row.get("part_data")?;
        let part_data: PartData = serde_json::from_str(&part_data_str)
            .unwrap_or_else(|_| PartData {
                r#type: "unknown".to_string(),
                text: None,
                tool: None,
                call_id: None,
                state: None,
                reason: None,
                time: None,
                done: None,
                body: None,
            });
        let snippet = part_data.text.clone().unwrap_or_default();

        Ok(SearchResult {
            session_id: row.get("session_id")?,
            session_title: row.get("title")?,
            time_created: row.get("time_created")?,
            message_id: row.get("message_id")?,
            msg_time_created: row.get("msg_time_created")?,
            snippet: truncate_with_context(&snippet, &query, 120),
        })
    })?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}

/// Get sessions grouped by project directory
pub fn get_report_summary(conn: &Connection, since_ts: i64, until_ts: i64) -> Result<ReportSummary> {
    let mut stmt = conn.prepare(
        "SELECT \
         COUNT(DISTINCT s.id), \
         COUNT(m.id), \
         COALESCE(SUM(CAST(json_extract(m.data, '$.tokens.total') AS INTEGER)), 0), \
         COALESCE(SUM(CAST(json_extract(m.data, '$.cost') AS REAL)), 0.0) \
         FROM session s \
         LEFT JOIN message m ON m.session_id = s.id \
         WHERE s.time_created >= ?1 AND s.time_created <= ?2"
    )?;
    let (total_sessions, total_messages, total_tokens, total_cost) =
        stmt.query_row([since_ts, until_ts], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, f64>(3)?,
            ))
        })?;
    Ok(ReportSummary {
        total_sessions,
        total_messages,
        total_tokens,
        total_cost,
        period_start: String::new(),
        period_end: String::new(),
    })
}

pub fn get_daily_trends(conn: &Connection, since_ts: i64, until_ts: i64) -> Result<Vec<DailyTrend>> {
    let sql = "SELECT \
        DATE(s.time_created / 1000, 'unixepoch') AS date, \
        COUNT(DISTINCT s.id) AS sessions, \
        COUNT(m.id) AS messages, \
        COALESCE(SUM(CAST(json_extract(m.data, '$.tokens.total') AS INTEGER)), 0) AS tokens, \
        COALESCE(SUM(CAST(json_extract(m.data, '$.cost') AS REAL)), 0.0) AS cost \
     FROM session s \
     LEFT JOIN message m ON m.session_id = s.id \
     WHERE s.time_created >= ?1 AND s.time_created <= ?2 \
     GROUP BY date ORDER BY date";
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([since_ts, until_ts], |row| {
        Ok(DailyTrend {
            date: row.get("date")?,
            sessions: row.get("sessions")?,
            messages: row.get("messages")?,
            tokens: row.get("tokens")?,
            cost: row.get("cost")?,
        })
    })?;
    let mut trends = Vec::new();
    for row in rows {
        trends.push(row?);
    }
    Ok(trends)
}

pub fn get_model_breakdown(conn: &Connection, since_ts: i64, until_ts: i64) -> Result<Vec<ModelBreakdown>> {
    let sql = "SELECT \
        COALESCE(json_extract(m.data, '$.model.modelID'), 'unknown') AS model, \
        COUNT(*) AS message_count, \
        COALESCE(SUM(CAST(json_extract(m.data, '$.tokens.total') AS INTEGER)), 0) AS total_tokens, \
        COALESCE(SUM(CAST(json_extract(m.data, '$.cost') AS REAL)), 0.0) AS total_cost \
     FROM message m \
     JOIN session s ON s.id = m.session_id \
     WHERE s.time_created >= ?1 AND s.time_created <= ?2 \
     GROUP BY model ORDER BY total_cost DESC";
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([since_ts, until_ts], |row| {
        Ok(ModelBreakdown {
            model: row.get("model")?,
            message_count: row.get("message_count")?,
            total_tokens: row.get("total_tokens")?,
            total_cost: row.get("total_cost")?,
        })
    })?;
    let mut breakdown = Vec::new();
    for row in rows {
        breakdown.push(row?);
    }
    Ok(breakdown)
}

pub fn list_projects(conn: &Connection, limit: i64) -> Result<Vec<ProjectGroup>> {
    let sql = "\
        SELECT s.directory, \
               COUNT(*) AS session_count, \
               SUM((SELECT COUNT(*) FROM message m WHERE m.session_id = s.id)) AS total_msgs, \
               MAX(s.time_created) AS last_active \
        FROM session s \
        GROUP BY s.directory \
        ORDER BY last_active DESC \
        LIMIT ?1";

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([limit], |row| {
        Ok(ProjectGroup {
            directory: row.get("directory")?,
            session_count: row.get("session_count")?,
            total_messages: row.get("total_msgs")?,
            last_active: row.get("last_active")?,
        })
    })?;
    let mut groups = Vec::new();
    for row in rows {
        groups.push(row?);
    }
    Ok(groups)
}

/// Rename a session
pub fn rename_session(conn: &Connection, id: &str, new_title: &str) -> Result<bool> {
    let sql = "UPDATE session SET title = ?1, time_updated = ?2 WHERE id = ?3";
    let now_ms = now_unix_ms();
    let affected = conn.execute(sql, rusqlite::params![new_title, now_ms, id])?;
    Ok(affected > 0)
}

/// Delete a session and its messages/parts
pub fn delete_session(conn: &Connection, id: &str) -> Result<bool> {
    conn.execute("DELETE FROM part WHERE session_id = ?1", rusqlite::params![id])?;
    conn.execute("DELETE FROM message WHERE session_id = ?1", rusqlite::params![id])?;
    let affected = conn.execute("DELETE FROM session WHERE id = ?1", rusqlite::params![id])?;
    Ok(affected > 0)
}

/// Read session diff from storage
pub fn read_session_diff(session_id: &str) -> Result<Vec<DiffEntry>> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/oc".to_string());
    let path = format!("{}/.local/share/opencode/storage/session_diff/{}.json", home, session_id);

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Vec::new());
        }
        Err(e) => anyhow::bail!("Failed to read diff file {}: {}", path, e),
    };

    let entries: Vec<DiffEntry> = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse diff JSON from {}", path))?;
    Ok(entries)
}

/// List sessions older than a given number of days
pub fn list_old_sessions(conn: &Connection, days: i64, limit: i64) -> Result<Vec<Session>> {
    let cutoff_ms = now_unix_ms() - days * 24 * 60 * 60 * 1000;

    let sql = "\
        SELECT s.id, s.project_id, s.slug, s.directory, s.title, \
               s.time_created, s.time_updated, \
               s.summary_additions, s.summary_deletions, s.summary_files, \
               s.parent_id, \
               (SELECT COUNT(*) FROM message m WHERE m.session_id = s.id) AS msg_count, \
               (SELECT json_extract(m.data, '$.model.modelID') FROM message m \
                WHERE m.session_id = s.id AND m.data LIKE '%model%' LIMIT 1) AS model, \
               COALESCE((SELECT SUM(json_extract(m.data, '$.cost')) FROM message m \
                WHERE m.session_id = s.id AND json_extract(m.data, '$.cost') IS NOT NULL), 0.0) AS total_cost \
         FROM session s \
         WHERE s.time_created < ?1 \
         ORDER BY s.time_created ASC \
         LIMIT ?2";

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(rusqlite::params![cutoff_ms, limit], |row| {
        Ok(Session {
            id: row.get("id")?,
            project_id: row.get("project_id")?,
            slug: row.get("slug")?,
            directory: row.get("directory")?,
            title: row.get("title")?,
            time_created: row.get("time_created")?,
            time_updated: row.get("time_updated")?,
            summary_additions: row.get("summary_additions")?,
            summary_deletions: row.get("summary_deletions")?,
            summary_files: row.get("summary_files")?,
            msg_count: row.get("msg_count")?,
            model: row.get("model")?,
            total_cost: row.get("total_cost")?,
            parent_id: row.get("parent_id")?,
        })
    })?;
    let mut sessions = Vec::new();
    for row in rows {
        sessions.push(row?);
    }
    Ok(sessions)
}

/// Get aggregate stats for a single session
pub fn get_session_stats(conn: &Connection, session_id: &str) -> Result<SessionStats> {
    let sql = "\
        SELECT m.data AS data, m.time_created \
        FROM message m \
        WHERE m.session_id = ?1 \
        ORDER BY m.time_created ASC";
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([session_id], |row| {
        let data_str: String = row.get("data")?;
        Ok(data_str)
    })?;

    let mut total_messages: i64 = 0;
    let mut user_messages: i64 = 0;
    let mut assistant_messages: i64 = 0;
    let mut total_tokens: i64 = 0;
    let mut input_tokens: i64 = 0;
    let mut output_tokens: i64 = 0;
    let mut reasoning_tokens: i64 = 0;
    let mut cache_write: i64 = 0;
    let mut cache_read: i64 = 0;
    let mut total_cost: f64 = 0.0;
    let mut agent_map: std::collections::HashMap<String, AgentBreakdown> = std::collections::HashMap::new();

    for row in rows {
        let data_str = row?;
        let data: MessageData = serde_json::from_str(&data_str).unwrap_or_else(|e| {
            eprintln!("Warning: failed to parse message.data JSON in get_stats: {e}");
            MessageData {
                role: "unknown".to_string(),
                agent: None,
                model: None,
                tokens: None,
                cost: None,
                mode: None,
                parent_id: None,
            }
        });
        total_messages += 1;
        match data.role.as_str() {
            "user" => user_messages += 1,
            "assistant" => assistant_messages += 1,
            _ => {}
        }
        if let Some(t) = &data.tokens {
            input_tokens += t.input.unwrap_or(0);
            output_tokens += t.output.unwrap_or(0);
            reasoning_tokens += t.reasoning.unwrap_or(0);
            total_tokens += t.total.unwrap_or(0);
            if let Some(c) = &t.cache {
                cache_write += c.write.unwrap_or(0);
                cache_read += c.read.unwrap_or(0);
            }
        }
        if let Some(c) = data.cost {
            total_cost += c;
        }
        let agent = data.agent.clone().unwrap_or_else(|| "unknown".to_string());
        let entry = agent_map.entry(agent.clone()).or_insert(AgentBreakdown {
            agent,
            message_count: 0,
            total_tokens: 0,
            total_cost: 0.0,
        });
        entry.message_count += 1;
        if let Some(t) = &data.tokens {
            entry.total_tokens += t.total.unwrap_or(0);
        }
        if let Some(c) = data.cost {
            entry.total_cost += c;
        }
    }

    let mut agent_breakdown: Vec<AgentBreakdown> = agent_map.into_values().collect();
    agent_breakdown.sort_by(|a, b| b.total_cost.partial_cmp(&a.total_cost).unwrap_or(std::cmp::Ordering::Equal));

    // Fetch session title
    let title = get_session(conn, session_id)?
        .map(|s| s.title)
        .unwrap_or_else(|| "Unknown".to_string());

    Ok(SessionStats {
        session_id: session_id.to_string(),
        session_title: title,
        total_messages,
        user_messages,
        assistant_messages,
        total_tokens,
        input_tokens,
        output_tokens,
        reasoning_tokens,
        cache_write,
        cache_read,
        total_cost,
        agent_breakdown,
    })
}

/// Top sessions by cost, tokens, or message count
pub fn top_sessions(conn: &Connection, limit: i64, sort_by: &str) -> Result<Vec<TopSessionEntry>> {
    let sql = match sort_by {
        "cost" => "\
            SELECT s.id, s.title, s.directory, s.time_created, \
                   COUNT(m.id) AS msg_count, \
                   COALESCE(SUM(CAST(json_extract(m.data, '$.cost') AS REAL)), 0) AS total_cost, \
                   COALESCE(SUM(CAST(json_extract(m.data, '$.tokens.total') AS INTEGER)), 0) AS total_tokens, \
                   COALESCE(SUM(CAST(json_extract(m.data, '$.tokens.input') AS INTEGER)), 0) AS total_input, \
                   COALESCE(SUM(CAST(json_extract(m.data, '$.tokens.output') AS INTEGER)), 0) AS total_output \
            FROM session s \
            JOIN message m ON m.session_id = s.id \
            GROUP BY s.id \
            ORDER BY total_cost DESC \
            LIMIT ?1",
        "tokens" => "\
            SELECT s.id, s.title, s.directory, s.time_created, \
                   COUNT(m.id) AS msg_count, \
                   COALESCE(SUM(CAST(json_extract(m.data, '$.cost') AS REAL)), 0) AS total_cost, \
                   COALESCE(SUM(CAST(json_extract(m.data, '$.tokens.total') AS INTEGER)), 0) AS total_tokens, \
                   COALESCE(SUM(CAST(json_extract(m.data, '$.tokens.input') AS INTEGER)), 0) AS total_input, \
                   COALESCE(SUM(CAST(json_extract(m.data, '$.tokens.output') AS INTEGER)), 0) AS total_output \
            FROM session s \
            JOIN message m ON m.session_id = s.id \
            GROUP BY s.id \
            ORDER BY total_tokens DESC \
            LIMIT ?1",
        "msgs" => "\
            SELECT s.id, s.title, s.directory, s.time_created, \
                   COUNT(m.id) AS msg_count, \
                   COALESCE(SUM(CAST(json_extract(m.data, '$.cost') AS REAL)), 0) AS total_cost, \
                   COALESCE(SUM(CAST(json_extract(m.data, '$.tokens.total') AS INTEGER)), 0) AS total_tokens, \
                   COALESCE(SUM(CAST(json_extract(m.data, '$.tokens.input') AS INTEGER)), 0) AS total_input, \
                   COALESCE(SUM(CAST(json_extract(m.data, '$.tokens.output') AS INTEGER)), 0) AS total_output \
            FROM session s \
            JOIN message m ON m.session_id = s.id \
            GROUP BY s.id \
            ORDER BY msg_count DESC \
            LIMIT ?1",
        _ => bail!("Invalid sort field: {sort_by}. Use 'cost', 'tokens', or 'msgs'."),
    };

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([limit], |row| {
        Ok(TopSessionEntry {
            id: row.get("id")?,
            title: row.get("title")?,
            directory: row.get("directory")?,
            time_created: row.get("time_created")?,
            msg_count: row.get("msg_count")?,
            total_cost: row.get("total_cost")?,
            total_tokens: row.get("total_tokens")?,
            total_input: row.get("total_input")?,
            total_output: row.get("total_output")?,
        })
    })?;
    let mut entries = Vec::new();
    for row in rows {
        entries.push(row?);
    }
    Ok(entries)
}

pub fn top_sessions_in_range(conn: &Connection, limit: i64, since_ts: i64, until_ts: i64) -> Result<Vec<TopSessionEntry>> {
    let sql = "\
        SELECT s.id, s.title, s.directory, s.time_created, \
               COUNT(m.id) AS msg_count, \
               COALESCE(SUM(CAST(json_extract(m.data, '$.cost') AS REAL)), 0) AS total_cost, \
               COALESCE(SUM(CAST(json_extract(m.data, '$.tokens.total') AS INTEGER)), 0) AS total_tokens, \
               COALESCE(SUM(CAST(json_extract(m.data, '$.tokens.input') AS INTEGER)), 0) AS total_input, \
               COALESCE(SUM(CAST(json_extract(m.data, '$.tokens.output') AS INTEGER)), 0) AS total_output \
        FROM session s \
        JOIN message m ON m.session_id = s.id \
        WHERE s.time_created >= ?1 AND s.time_created <= ?2 \
        GROUP BY s.id \
        ORDER BY total_cost DESC \
        LIMIT ?3";

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(rusqlite::params![since_ts, until_ts, limit], |row| {
        Ok(TopSessionEntry {
            id: row.get("id")?,
            title: row.get("title")?,
            directory: row.get("directory")?,
            time_created: row.get("time_created")?,
            msg_count: row.get("msg_count")?,
            total_cost: row.get("total_cost")?,
            total_tokens: row.get("total_tokens")?,
            total_input: row.get("total_input")?,
            total_output: row.get("total_output")?,
        })
    })?;
    let mut entries = Vec::new();
    for row in rows {
        entries.push(row?);
    }
    Ok(entries)
}

fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as i64
}

fn truncate_with_context(text: &str, query: &str, context_chars: usize) -> String {
    let lower_query = query.to_lowercase();
    let lower_text = text.to_lowercase();
    if let Some(byte_pos) = lower_text.find(&lower_query) {
        // Count chars in lower_text (safe byte_pos) to get char position
        let char_pos = lower_text[..byte_pos].chars().count();
        let query_chars = query.chars().count();
        let total_chars = text.chars().count();
        let half = context_chars / 2;

        let start_char = char_pos.saturating_sub(half);
        let end_char = std::cmp::min(char_pos + query_chars + half, total_chars);

        let mut result = String::new();
        if start_char > 0 {
            result.push_str("...");
        }
        result.extend(text.chars().skip(start_char).take(end_char - start_char));
        if end_char < total_chars {
            result.push_str("...");
        }
        result
    } else {
        let truncated: String = text.chars().take(context_chars).collect();
        if text.chars().count() > context_chars {
            format!("{}...", truncated)
        } else {
            truncated
        }
    }
}

// ── FTS5 Full-Text Search ──────────────────────────────────────────────

/// Path to the FTS5 sidecar database
pub fn fts_db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/oc".to_string());
    PathBuf::from(format!("{}/.local/share/opencode/ocs_fts.db", home))
}

/// Open (or create) the FTS5 sidecar database with the virtual table schema
pub fn open_fts_db() -> Result<Connection> {
    let path = fts_db_path();
    let conn = Connection::open(&path)
        .with_context(|| format!("Failed to open FTS database at {:?}", path))?;
    conn.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS message_fts USING fts5(
            session_id UNINDEXED,
            message_id UNINDEXED,
            msg_time_created UNINDEXED,
            session_title,
            content,
            tokenize='unicode61'
        );"
    )?;
    Ok(conn)
}

/// Check if the FTS database exists and has data
pub fn fts_db_ready() -> bool {
    let path = fts_db_path();
    if !path.exists() {
        return false;
    }
    match Connection::open(&path) {
        Ok(conn) => {
            conn.query_row(
                "SELECT COUNT(*) FROM message_fts",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count > 0)
            .unwrap_or(false)
        }
        Err(_) => false,
    }
}

/// Rebuild the FTS index from the opencode database
///
/// Iterates all sessions, extracts message text bodies, and inserts into FTS5.
/// Returns the number of messages indexed.
pub fn rebuild_fts_index(conn: &Connection) -> Result<usize> {
    let fts_conn = open_fts_db()?;
    fts_conn.execute("DELETE FROM message_fts", [])?;

    // Collect all session IDs and titles
    let mut stmt = conn.prepare("SELECT id, title FROM session ORDER BY time_created ASC")?;
    let mut skipped_sessions = 0usize;
    let sessions: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .filter_map(|r| {
            match r {
                Ok(row) => Some(row),
                Err(e) => {
                    eprintln!("Warning: rebuild_fts_index: skipping session row: {e}");
                    skipped_sessions += 1;
                    None
                }
            }
        })
        .collect();
    if skipped_sessions > 0 {
        eprintln!("Warning: skipped {skipped_sessions} session(s) during FTS index rebuild");
    }

    let mut total = 0usize;

    for (session_id, session_title) in &sessions {
        let messages = get_messages(conn, session_id)?;
        if messages.is_empty() {
            continue;
        }

        let msg_ids: Vec<String> = messages.iter().map(|m| m.id.clone()).collect();
        let parts_map = get_parts_batch(conn, &msg_ids)?;

        let fts_conn = open_fts_db()?;

        // Use a transaction per session for performance
        fts_conn.execute_batch("BEGIN TRANSACTION")?;

        for msg in &messages {
            let parts = parts_map
                .iter()
                .find(|(mid, _)| **mid == msg.id)
                .map(|(_, parts)| parts.clone())
                .unwrap_or_default();

            let mwp = crate::models::MessageWithParts {
                message: msg.clone(),
                parts,
            };
            let body = mwp.text_body();
            if body.is_empty() {
                continue;
            }

            fts_conn.execute(
                "INSERT INTO message_fts (session_id, message_id, msg_time_created, session_title, content) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![session_id, msg.id, msg.time_created, session_title, body],
            )?;
            total += 1;
        }

        fts_conn.execute_batch("COMMIT")?;
    }

    Ok(total)
}

/// Build a simple FTS5 query from a user-provided search string
fn to_fts_query(user_query: &str) -> String {
    let terms: Vec<String> = user_query
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}*\"", t.replace('"', "")))
        .collect();
    if terms.is_empty() {
        return String::new();
    }
    terms.join(" AND ")
}

/// Search using the FTS5 index (faster, ranked results)
pub fn search_fts(conn: &Connection, query: &str, limit: i64) -> Result<Vec<SearchResult>> {
    if !fts_db_ready() {
        return search_sessions(conn, query, limit);
    }

    let fts_conn = open_fts_db()?;
    let fts_query = to_fts_query(query);
    if fts_query.is_empty() {
        return Ok(Vec::new());
    }

    let sql = format!(
        "SELECT session_id, message_id, msg_time_created, session_title, \
                highlight(message_fts, 4, '**', '**') AS snippet \
         FROM message_fts \
         WHERE message_fts MATCH ?1 \
         ORDER BY rank \
         LIMIT ?2"
    );

    let mut stmt = fts_conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params![fts_query, limit], |row| {
        Ok(SearchResult {
            session_id: row.get("session_id")?,
            session_title: row.get("session_title")?,
            time_created: row.get("msg_time_created")?,
            message_id: row.get("message_id")?,
            msg_time_created: row.get("msg_time_created")?,
            snippet: row.get("snippet")?,
        })
    })?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}
