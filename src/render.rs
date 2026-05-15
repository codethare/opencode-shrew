use chrono::DateTime;

use crate::models::*;

/// Render list of sessions as markdown
pub fn render_session_list(sessions: &[Session]) -> String {
    let mut out = String::new();
    out.push_str("# OpenCode Sessions\n\n");
    for s in sessions {
        let created_ts = ts_to_iso(s.time_created);
        let model = s.model.as_deref().unwrap_or("unknown");
        let diff_summary = match (s.summary_additions, s.summary_deletions) {
            (Some(a), Some(d)) if a > 0 || d > 0 => format!(" (+{}/-{})", a, d),
            _ => String::new(),
        };
        out.push_str(&format!("## [`{}`](opencode://session/{})  \n", s.id, s.id));
        out.push_str(&format!("**{}** | `{}` messages | Model: `{}`  \n", s.title, s.msg_count, model));
        out.push_str(&format!("📁 {}  \n", s.directory));
        out.push_str(&format!("🕐 {}{}  \n\n", created_ts, diff_summary));
    }
    out
}

/// Render list of sessions as compact one-liners (for terminal / peco)
pub fn render_session_list_compact(sessions: &[Session]) -> Vec<String> {
    sessions
        .iter()
        .map(|s| {
            let created_ts = ts_to_compact(s.time_created);
            let model = s.model.as_deref().unwrap_or("unknown");
            format!("{}\t{}\t{}\t{}\t{}", s.id, created_ts, s.msg_count, model, s.title)
        })
        .collect()
}

/// Render a full session with its messages as markdown
pub fn render_session_detail(session: &Session, messages: &[MessageWithParts], show_tools: bool) -> String {
    let mut out = String::new();
    let model = session.model.as_deref().unwrap_or("unknown");
    let created_ts = ts_to_iso_short(session.time_created);
    let updated_ts = ts_to_iso_short(session.time_updated);

    out.push_str(&format!("# {}\n\n", session.title));
    out.push_str("---\n\n");
    out.push_str(&format!("**Session**: `{}`  \n", session.id));
    out.push_str(&format!("**Model**: `{}`  \n", model));
    out.push_str(&format!("**Directory**: `{}`  \n", session.directory));
    out.push_str(&format!("**Created**: {}  \n", created_ts));
    out.push_str(&format!("**Updated**: {}  \n", updated_ts));
    out.push_str(&format!("**Messages**: {}  \n", session.msg_count));

    if let Some(files) = session.summary_files {
        if files > 0 {
            out.push_str(&format!("**Files changed**: {} (+{}/-{})  \n", files,
                session.summary_additions.unwrap_or(0),
                session.summary_deletions.unwrap_or(0)));
        }
    }
    out.push_str("\n---\n\n");

    for msg_with_parts in messages {
        render_message(&mut out, msg_with_parts, show_tools);
    }
    out
}

fn render_message(out: &mut String, msg: &MessageWithParts, show_tools: bool) {
    let ts = ts_to_time(msg.message.time_created);
    let agent = msg.message.data.agent.as_deref().unwrap_or("unknown");
    let tokens = msg.message.data.tokens.as_ref();
    let cost = msg.message.data.cost;

    match msg.message.data.role.as_str() {
        "user" => {
            out.push_str(&format!("## 🧑 User ({ts})\n\n"));
            let body = msg.text_body();
            if !body.is_empty() {
                out.push_str(&body);
                out.push('\n');
            }
        }
        "assistant" => {
            let mut header = format!("## 🤖 {} ({ts})", agent);
            if let Some(t) = tokens {
                if let Some(total) = t.total {
                    header.push_str(&format!(" · {} tokens", total));
                }
            }
            if let Some(c) = cost {
                if c > 0.0 {
                    header.push_str(&format!(" · ${:.6}", c));
                }
            }
            header.push_str("\n\n");
            out.push_str(&header);

            if let Some(reasoning) = msg.reasoning_body() {
                out.push_str("<details>\n<summary>💭 Reasoning</summary>\n\n");
                for line in reasoning.lines() {
                    out.push_str("> ");
                    out.push_str(line);
                    out.push('\n');
                }
                out.push('\n');
                out.push_str("</details>\n\n");
            }

            if show_tools {
                for tool_part in msg.tool_calls() {
                    render_tool_call(out, tool_part);
                }
            }

            let body = msg.text_body();
            if !body.is_empty() {
                out.push_str(&body);
                out.push('\n');
            }

            if let Some(finish) = msg.step_finish() {
                if let Some(reason) = &finish.data.reason {
                    out.push_str(&format!("\n> ⏹ Finished: `{}`\n\n", reason));
                }
            }
        }
        _ => {
            let body = msg.text_body();
            out.push_str(&format!("## {} ({ts})\n\n", msg.message.data.role));
            if !body.is_empty() {
                out.push_str(&body);
                out.push('\n');
            }
        }
    }
    out.push_str("---\n\n");
}

fn render_tool_call(out: &mut String, part: &Part) {
    let tool_name = part.data.tool.as_deref().unwrap_or("unknown");
    let status = part.data.state.as_ref().map(|s| s.status.as_str()).unwrap_or("unknown");

    out.push_str(&format!("**🛠 Tool: `{}`** (status: `{}`)  \n", tool_name, status));

    if tool_name == "bash" {
        if let Some(state) = &part.data.state {
            if let Some(input) = &state.input {
                if let Some(cmd) = input.get("command").and_then(|v| v.as_str()) {
                    out.push_str(&format!("```bash\n{}\n```\n\n", cmd));
                }
            }
            if let Some(output) = &state.output {
                let trimmed = if output.len() > 2000 {
                    format!("{}...\n*(output truncated to 2000 chars)*", &output[..2000])
                } else {
                    output.clone()
                };
                out.push_str("**Output:**\n");
                out.push_str(&format!("```\n{}\n```\n\n", trimmed));
            }
        }
    } else if let Some(state) = &part.data.state {
        if let Some(input) = &state.input {
            if let Ok(input_str) = serde_json::to_string_pretty(input) {
                out.push_str(&format!("**Input:**\n```json\n{}\n```\n\n", input_str));
            }
        }
        if let Some(output) = &state.output {
            let trimmed = if output.len() > 2000 {
                format!("{}...\n*(output truncated)*", &output[..2000])
            } else {
                output.clone()
            };
            out.push_str(&format!("**Output:**\n```\n{}\n```\n\n", trimmed));
        }
    }
}

/// Render raw JSON output for a session
pub fn render_session_raw(session: &Session, messages: &[MessageWithParts]) -> Result<String, serde_json::Error> {
    #[derive(serde::Serialize)]
    struct RawOutput<'a> {
        session: &'a Session,
        messages: Vec<RawMessage<'a>>,
    }
    #[derive(serde::Serialize)]
    struct RawMessage<'a> {
        id: &'a str,
        role: &'a str,
        agent: Option<&'a str>,
        time_created: i64,
        tokens: Option<&'a TokenUsage>,
        cost: Option<f64>,
        parts: Vec<RawPart<'a>>,
    }
    #[derive(serde::Serialize)]
    struct RawPart<'a> {
        id: &'a str,
        r#type: &'a str,
        text: Option<&'a str>,
        tool: Option<&'a str>,
        state: Option<&'a ToolState>,
    }
    let raw_msgs: Vec<RawMessage> = messages.iter().map(|m| RawMessage {
        id: &m.message.id,
        role: &m.message.data.role,
        agent: m.message.data.agent.as_deref(),
        time_created: m.message.time_created,
        tokens: m.message.data.tokens.as_ref(),
        cost: m.message.data.cost,
        parts: m.parts.iter().map(|p| RawPart {
            id: &p.id,
            r#type: &p.data.r#type,
            text: p.data.text.as_deref(),
            tool: p.data.tool.as_deref(),
            state: p.data.state.as_ref(),
        }).collect(),
    }).collect();
    serde_json::to_string_pretty(&RawOutput { session, messages: raw_msgs })
}

/// Render session diff output
pub fn render_diff(entries: &[DiffEntry], session_id: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Diff: `{}`\n\n", session_id));

    if entries.is_empty() {
        out.push_str("No file changes recorded for this session.\n");
        return out;
    }

    for entry in entries {
        let status = entry.status.as_deref().unwrap_or("unknown");
        let adds = entry.additions.unwrap_or(0);
        let dels = entry.deletions.unwrap_or(0);
        out.push_str(&format!("## `{}`  \n", entry.file));
        out.push_str(&format!("**Status**: {} · **+{}/-{}**  \n\n", status, adds, dels));

        if let Some(patch) = &entry.patch {
            out.push_str(&format!("```diff\n{}\n```\n\n", patch));
        }
    }
    out
}

/// Render prune plan for confirmation
pub fn render_prune_plan(sessions: &[Session], days: i64) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Prune Plan: sessions older than {} days\n\n", days));
    out.push_str(&format!("**{} session(s) will be deleted.**\n\n", sessions.len()));

    if sessions.is_empty() {
        return out;
    }

    out.push_str("| Session | Title | Messages | Created |\n|---|---|---|---|\n");
    for s in sessions {
        let ts = ts_to_compact(s.time_created);
        let short_id = if s.id.len() > 16 {
            format!("{}…", &s.id[..16])
        } else {
            s.id.clone()
        };
        out.push_str(&format!("| `{}` | {} | {} | {} |\n",
            short_id, s.title, s.msg_count, ts));
    }
    out
}

/// Render project directory listing
pub fn render_projects(groups: &[ProjectGroup]) -> String {
    let mut out = String::new();
    out.push_str("# Projects\n\n");
    out.push_str("| # | Directory | Sessions | Messages | Last Active |\n|---|---|---|---|---|\n");
    for (i, g) in groups.iter().enumerate() {
        let ts = ts_to_compact(g.last_active);
        out.push_str(&format!("| {} | `{}` | {} | {} | {} |\n",
            i + 1, g.directory, g.session_count, g.total_messages, ts));
    }
    out
}

/// Render aggregated stats for a session
pub fn render_session_stats(stats: &SessionStats) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Stats: {}\n\n", stats.session_title));
    out.push_str(&format!("**Session**: `{}`  \n", stats.session_id));
    out.push_str("---\n\n");

    // Message counts
    out.push_str("## Messages\n\n");
    out.push_str(&format!("| | Count |\n|---|---|\n"));
    out.push_str(&format!("| **Total** | {} |\n", stats.total_messages));
    out.push_str(&format!("| User | {} |\n", stats.user_messages));
    out.push_str(&format!("| Assistant | {} |\n", stats.assistant_messages));
    out.push('\n');

    // Token usage
    out.push_str("## Token Usage\n\n");
    out.push_str("| Metric | Tokens |\n|---|---|\n");
    out.push_str(&format!("| **Total** | {} |\n", stats.total_tokens));
    out.push_str(&format!("| Input | {} |\n", stats.input_tokens));
    out.push_str(&format!("| Output | {} |\n", stats.output_tokens));
    out.push_str(&format!("| Reasoning | {} |\n", stats.reasoning_tokens));
    if stats.cache_write > 0 || stats.cache_read > 0 {
        out.push_str(&format!("| Cache Write | {} |\n", stats.cache_write));
        out.push_str(&format!("| Cache Read | {} |\n", stats.cache_read));
    }
    out.push('\n');

    // Cost
    out.push_str("## Cost\n\n");
    out.push_str(&format!("**Total cost**: ${:.6}\n\n", stats.total_cost));

    // Per-agent breakdown
    if !stats.agent_breakdown.is_empty() {
        out.push_str("## Per-Agent Breakdown\n\n");
        out.push_str("| Agent | Messages | Tokens | Cost |\n|---|---|---|---|\n");
        for a in &stats.agent_breakdown {
            out.push_str(&format!("| {} | {} | {} | ${:.6} |\n",
                a.agent, a.message_count, a.total_tokens, a.total_cost));
        }
        out.push('\n');
    }

    out
}

/// Render search results
pub fn render_search_results(results: &[SearchResult], query: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Search Results for \"{query}\"\n\n", query = query.replace('"', "\\\"")));

    if results.is_empty() {
        out.push_str("No results found.\n");
        return out;
    }

    // Group by session
    let mut by_session: std::collections::BTreeMap<String, Vec<&SearchResult>> = std::collections::BTreeMap::new();
    for r in results {
        by_session.entry(r.session_id.clone()).or_default().push(r);
    }

    for (session_id, hits) in &by_session {
        let first = hits[0];
        let ts = ts_to_compact(first.time_created);
        out.push_str(&format!("## [`{}`](opencode://session/{})  \n", first.session_title, session_id));
        out.push_str(&format!("📁 Session: `{}` · {} · {} match(es)  \n", session_id, ts, hits.len()));
        out.push('\n');

        for hit in hits {
            let msg_ts = ts_to_time(hit.msg_time_created);
            let snippet = &hit.snippet;
            // Highlight the query term
            let highlighted = snippet.replace(query, &format!("**{}**", query));
            out.push_str(&format!("> 🕐 {} · {}\n\n", msg_ts, highlighted));
        }
        out.push('\n');
    }
    out
}

/// Render top sessions table
pub fn render_top_sessions(entries: &[TopSessionEntry], sort_by: &str) -> String {
    let mut out = String::new();
    let title = match sort_by {
        "cost" => "Top Sessions by Cost",
        "tokens" => "Top Sessions by Token Usage",
        "msgs" => "Top Sessions by Message Count",
        _ => "Top Sessions",
    };
    out.push_str(&format!("# {}\n\n", title));
    out.push_str("| # | Session | Messages | Tokens (in/out) | Cost | Last Active |\n|---|---|---|---|---|---|\n");

    for (i, e) in entries.iter().enumerate() {
        let ts = ts_to_compact(e.time_created);
        let short_id = if e.id.len() > 20 {
            format!("{}…", &e.id[..20])
        } else {
            e.id.clone()
        };
        out.push_str(&format!("| {} | `{}` {} | {} | {}/{} | ${:.6} | {} |\n",
            i + 1,
            short_id,
            e.title,
            e.msg_count,
            e.total_input,
            e.total_output,
            e.total_cost,
            ts,
        ));
    }
    out
}

fn ts_to_iso(ts: i64) -> String {
    let secs = ts / 1000;
    let nsecs = ((ts % 1000) * 1_000_000) as u32;
    match DateTime::from_timestamp(secs, nsecs) {
        Some(dt) => dt.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        None => format!("(invalid timestamp {})", ts),
    }
}

fn ts_to_iso_short(ts: i64) -> String {
    let secs = ts / 1000;
    let nsecs = ((ts % 1000) * 1_000_000) as u32;
    match DateTime::from_timestamp(secs, nsecs) {
        Some(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
        None => format!("(invalid timestamp {})", ts),
    }
}

fn ts_to_compact(ts: i64) -> String {
    let secs = ts / 1000;
    let nsecs = ((ts % 1000) * 1_000_000) as u32;
    match DateTime::from_timestamp(secs, nsecs) {
        Some(dt) => dt.format("%m-%d %H:%M").to_string(),
        None => String::new(),
    }
}

fn ts_to_time(ts: i64) -> String {
    let secs = ts / 1000;
    let nsecs = ((ts % 1000) * 1_000_000) as u32;
    match DateTime::from_timestamp(secs, nsecs) {
        Some(dt) => dt.format("%H:%M:%S").to_string(),
        None => String::new(),
    }
}
