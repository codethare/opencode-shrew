use chrono::DateTime;

use crate::models::*;
use std::fmt::Write;

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
                    let mut end = 2000;
                    while !output.is_char_boundary(end) {
                        end -= 1;
                    }
                    format!("{}...\n*(output truncated to 2000 chars)*", &output[..end])
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
                let mut end = 2000;
                while !output.is_char_boundary(end) {
                    end -= 1;
                }
                format!("{}...\n*(output truncated)*", &output[..end])
            } else {
                output.clone()
            };
            out.push_str(&format!("**Output:**\n```\n{}\n```\n\n", trimmed));
        }
    }
}

/// Render raw JSON output for a session
pub fn render_session_raw(session: &Session, messages: &[MessageWithParts], sanitize: bool) -> Result<String, serde_json::Error> {
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
        parts: Vec<Part>,
    }

    let raw_msgs: Vec<RawMessage> = messages.iter().map(|m| {
        let parts: Vec<Part> = m.parts.iter().map(|p| {
            let mut part = p.clone();
            if sanitize {
                if part.data.r#type == "text" || part.data.r#type == "reasoning" {
                    part.data.text = Some("[redacted]".to_string());
                }
                if let Some(state) = &mut part.data.state {
                    state.input = None;
                    state.output = Some("[redacted]".to_string());
                }
            }
            part
        }).collect();

        RawMessage {
            id: &m.message.id,
            role: &m.message.data.role,
            agent: m.message.data.agent.as_deref(),
            time_created: m.message.time_created,
            tokens: m.message.data.tokens.as_ref(),
            cost: m.message.data.cost,
            parts,
        }
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
        let short_id = if s.id.chars().count() > 16 {
            format!("{}…", s.id.chars().take(16).collect::<String>())
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
        let short_id = if e.id.chars().count() > 20 {
            format!("{}…", e.id.chars().take(20).collect::<String>())
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

/// Render session as markdown with YAML frontmatter for export
pub fn render_export_markdown(session: &Session, messages: &[MessageWithParts], show_tools: bool) -> String {
    let model = session.model.as_deref().unwrap_or("unknown");
    let created_ts = ts_to_iso(session.time_created);
    let updated_ts = ts_to_iso(session.time_updated);
    let total_cost: f64 = messages.iter()
        .filter_map(|m| m.message.data.cost)
        .sum();

    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("id: \"{}\"\n", session.id));
    out.push_str(&format!("title: \"{}\"\n", session.title));
    out.push_str(&format!("model: \"{}\"\n", model));
    out.push_str(&format!("directory: \"{}\"\n", session.directory));
    out.push_str(&format!("created: \"{}\"\n", created_ts));
    out.push_str(&format!("updated: \"{}\"\n", updated_ts));
    out.push_str(&format!("messages: {}\n", session.msg_count));
    out.push_str(&format!("cost: {}\n", total_cost));
    if let Some(files) = session.summary_files {
        if files > 0 {
            out.push_str(&format!("files_changed: {}\n", files));
            out.push_str(&format!("additions: {}\n", session.summary_additions.unwrap_or(0)));
            out.push_str(&format!("deletions: {}\n", session.summary_deletions.unwrap_or(0)));
        }
    }
    out.push_str("---\n\n");

    out.push_str(&render_session_detail(session, messages, show_tools));
    out
}

/// Render session as JSON for export
pub fn render_export_json(session: &Session, messages: &[MessageWithParts], sanitize: bool) -> Result<String, serde_json::Error> {
    #[derive(serde::Serialize)]
    struct ExportOutput<'a> {
        session: &'a Session,
        messages: Vec<ExportMessage<'a>>,
        stats: ExportStats,
    }
    #[derive(serde::Serialize)]
    struct ExportMessage<'a> {
        id: &'a str,
        role: &'a str,
        agent: Option<&'a str>,
        model: Option<&'a ModelInfo>,
        time_created: i64,
        tokens: Option<&'a TokenUsage>,
        cost: Option<f64>,
        parts: Vec<Part>,
    }
    #[derive(serde::Serialize)]
    struct ExportStats {
        total_messages: usize,
        total_tokens: i64,
        total_cost: f64,
        user_messages: usize,
        assistant_messages: usize,
    }

    let total_messages = messages.len();
    let user_messages = messages.iter().filter(|m| m.message.data.role == "user").count();
    let assistant_messages = messages.iter().filter(|m| m.message.data.role == "assistant").count();
    let total_tokens: i64 = messages.iter()
        .filter_map(|m| m.message.data.tokens.as_ref())
        .filter_map(|t| t.total)
        .sum();
    let total_cost: f64 = messages.iter()
        .filter_map(|m| m.message.data.cost)
        .sum();

    let exp_msgs: Vec<ExportMessage> = messages.iter().map(|m| {
        let parts: Vec<Part> = m.parts.iter().map(|p| {
            let mut part = p.clone();
            if sanitize {
                if part.data.r#type == "text" || part.data.r#type == "reasoning" {
                    part.data.text = Some("[redacted]".to_string());
                }
                if let Some(state) = &mut part.data.state {
                    state.input = None;
                    state.output = Some("[redacted]".to_string());
                }
            }
            part
        }).collect();

        ExportMessage {
            id: &m.message.id,
            role: &m.message.data.role,
            agent: m.message.data.agent.as_deref(),
            model: m.message.data.model.as_ref(),
            time_created: m.message.time_created,
            tokens: m.message.data.tokens.as_ref(),
            cost: m.message.data.cost,
            parts,
        }
    }).collect();

    let stats = ExportStats {
        total_messages,
        total_tokens,
        total_cost,
        user_messages,
        assistant_messages,
    };

    serde_json::to_string_pretty(&ExportOutput { session, messages: exp_msgs, stats })
}

/// Render a usage report as markdown
pub fn render_report_markdown(summary: &ReportSummary, trends: &[DailyTrend], models: &[ModelBreakdown], top_sessions: &[TopSessionEntry]) -> String {
    let mut out = String::new();
    out.push_str(&format!("# OpenCode 使用报告\n\n"));
    out.push_str(&format!("**期间:** {} ~ {}\n\n", summary.period_start, summary.period_end));
    out.push_str("## 总览\n\n");
    out.push_str(&format!("| 指标 | 数值 |\n"));
    out.push_str(&format!("|------|------|\n"));
    out.push_str(&format!("| 会话数 | {} |\n", summary.total_sessions));
    out.push_str(&format!("| 总消息 | {} |\n", summary.total_messages));
    out.push_str(&format!("| 总 Tokens | {} |\n", summary.total_tokens));
    out.push_str(&format!("| 总费用 | ${:.4} |\n", summary.total_cost));

    if !top_sessions.is_empty() {
        out.push_str("\n## 会话排行 (Top 10 by Cost)\n\n");
        out.push_str("| # | 会话 | 标题 | 消息 | Tokens | 费用 |\n");
        out.push_str("|---|------|------|------|--------|------|\n");
        for (i, s) in top_sessions.iter().enumerate() {
            let short_id = if s.id.chars().count() > 12 {
                format!("{}…", s.id.chars().take(12).collect::<String>())
            } else {
                s.id.clone()
            };
            let title_escaped = s.title.replace('|', "\\|");
            out.push_str(&format!("| {} | {} | {} | {} | {} | ${:.4} |\n",
                i + 1, short_id, title_escaped, s.msg_count, s.total_tokens, s.total_cost));
        }
    }

    if !trends.is_empty() {
        out.push_str("\n## 每日趋势\n\n");
        out.push_str("| 日期 | 会话 | 消息 | Tokens | 费用 |\n");
        out.push_str("|------|------|------|--------|------|\n");
        for t in trends {
            out.push_str(&format!("| {} | {} | {} | {} | ${:.4} |\n",
                t.date, t.sessions, t.messages, t.tokens, t.cost));
        }
        out.push_str("\n");
        let max_cost = trends.iter().map(|t| t.cost).fold(0.0_f64, f64::max);
        if max_cost > 0.0 {
            for t in trends {
                let bar_len = (t.cost / max_cost * 40.0) as usize;
                let bar = "█".repeat(bar_len);
                out.push_str(&format!("{}: {} ${:.4}\n", t.date, bar, t.cost));
            }
        }
    }

    if !models.is_empty() {
        out.push_str("\n## 模型使用分布\n\n");
        out.push_str("| 模型 | 消息数 | Tokens | 费用 | 占比 |\n");
        out.push_str("|------|--------|--------|------|------|\n");
        let total_model_cost: f64 = models.iter().map(|m| m.total_cost).sum();
        for m in models {
            let pct = if total_model_cost > 0.0 {
                format!("{:.1}%", m.total_cost / total_model_cost * 100.0)
            } else {
                "-".to_string()
            };
            out.push_str(&format!("| {} | {} | {} | ${:.4} | {} |\n",
                m.model, m.message_count, m.total_tokens, m.total_cost, pct));
        }
    }

    out
}

/// Render a usage report as JSON
pub fn render_report_json(summary: &ReportSummary, trends: &[DailyTrend], models: &[ModelBreakdown], top_sessions: &[TopSessionEntry]) -> Result<String, serde_json::Error> {
    #[derive(serde::Serialize)]
    struct ReportOutput<'a> {
        summary: &'a ReportSummary,
        daily_trends: &'a [DailyTrend],
        model_breakdown: &'a [ModelBreakdown],
        top_sessions: &'a [TopSessionEntry],
    }
    serde_json::to_string_pretty(&ReportOutput {
        summary,
        daily_trends: trends,
        model_breakdown: models,
        top_sessions,
    })
}

/// Render a single message without session header (for watch mode)
pub fn render_message_only(msg: &MessageWithParts, show_tools: bool) -> String {
    let mut out = String::new();
    render_message(&mut out, msg, show_tools);
    out
}

/// Render a single message in compact one-line format (for watch mode)
pub fn render_message_compact(msg: &MessageWithParts) -> String {
    let ts = ts_to_time(msg.message.time_created);
    let agent = msg.message.data.agent.as_deref().unwrap_or("unknown");
    match msg.message.data.role.as_str() {
        "user" => {
            let body = msg.text_body();
            let preview: String = body.chars().take(80).collect();
            if body.len() > 80 {
                format!("[{ts}] 🧑 User: {}…", preview)
            } else {
                format!("[{ts}] 🧑 User: {}", preview)
            }
        }
        "assistant" => {
            let tokens = msg.message.data.tokens.as_ref()
                .and_then(|t| t.total)
                .map(|t| format!(" · {} tokens", t))
                .unwrap_or_default();
            let cost = msg.message.data.cost
                .filter(|c| *c > 0.0)
                .map(|c| format!(" · ${:.6}", c))
                .unwrap_or_default();
            let body = msg.text_body();
            let preview: String = body.chars().take(80).collect();
            if body.len() > 80 {
                format!("[{ts}] 🤖 {agent}{}{}: {}…", tokens, cost, preview)
            } else {
                format!("[{ts}] 🤖 {agent}{}{}: {}", tokens, cost, preview)
            }
        }
        _ => String::new(),
    }
}

fn ts_to_iso(ts: i64) -> String {
    let secs = ts / 1000;
    let nsecs = ((ts % 1000) * 1_000_000) as u32;
    match DateTime::from_timestamp(secs, nsecs) {
        Some(dt) => dt.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        None => format!("(invalid timestamp {})", ts),
    }
}

pub fn render_compare_markdown(
    s1: &Session,
    s2: &Session,
    mp1: &[MessageWithParts],
    mp2: &[MessageWithParts],
    _stats_only: bool,
) -> Result<String, anyhow::Error> {
    let mut out = String::new();
    out.push_str("# 会话对比\n\n");

    let agg1 = aggregate_usage(mp1);
    let agg2 = aggregate_usage(mp2);

    // Metadata comparison table
    out.push_str("## 元数据对比\n\n");
    out.push_str("| 指标 | 会话 1 | 会话 2 |\n");
    out.push_str("|------|--------|--------|\n");
    cell_str(&mut out, "ID", &s1.id, &s2.id);
    cell_str(&mut out, "标题", &s1.title, &s2.title);
    cell_str(&mut out, "模型", s1.model.as_deref().unwrap_or("—"), s2.model.as_deref().unwrap_or("—"));
    cell_usize(&mut out, "消息数", mp1.len(), mp2.len());
    cell_str(&mut out, "创建时间", &ts_to_iso_short(s1.time_created), &ts_to_iso_short(s2.time_created));
    cell_str(&mut out, "目录", &s1.directory, &s2.directory);

    // Token / Cost comparison
    out.push_str("\n## Token / 费用\n\n");
    out.push_str("| 指标 | 会话 1 | 会话 2 |\n");
    out.push_str("|------|--------|--------|\n");
    cell_i64(&mut out, "总 Tokens", agg1.total_tokens, agg2.total_tokens);
    cell_i64(&mut out, "输入 Tokens", agg1.input_tokens, agg2.input_tokens);
    cell_i64(&mut out, "输出 Tokens", agg1.output_tokens, agg2.output_tokens);
    writeln!(out, "| 总费用 | ${:.4} | ${:.4} |", agg1.total_cost, agg2.total_cost).ok();

    // Agent usage
    out.push_str("\n## 智能体使用\n\n");
    let all_agents: std::collections::BTreeSet<&String> = agg1.agent_counts.keys()
        .chain(agg2.agent_counts.keys())
        .collect();
    out.push_str("| Agent | 会话 1 消息 | 会话 2 消息 |\n");
    out.push_str("|-------|-----------|-----------|\n");
    for agent in all_agents {
        let c1 = agg1.agent_counts.get(agent).map(|v| v.to_string()).unwrap_or_else(|| "—".to_string());
        let c2 = agg2.agent_counts.get(agent).map(|v| v.to_string()).unwrap_or_else(|| "—".to_string());
        writeln!(out, "| {} | {} | {} |", agent, c1, c2).ok();
    }

    // File changes
    out.push_str("\n## 文件变更\n\n");
    out.push_str("| 指标 | 会话 1 | 会话 2 |\n");
    out.push_str("|------|--------|--------|\n");
    cell_i64(&mut out, "新增行", s1.summary_additions.unwrap_or(0), s2.summary_additions.unwrap_or(0));
    cell_i64(&mut out, "删除行", s1.summary_deletions.unwrap_or(0), s2.summary_deletions.unwrap_or(0));

    Ok(out)
}

struct UsageAgg {
    total_tokens: i64,
    input_tokens: i64,
    output_tokens: i64,
    total_cost: f64,
    agent_counts: std::collections::BTreeMap<String, usize>,
}

fn aggregate_usage(messages: &[MessageWithParts]) -> UsageAgg {
    let mut total_tokens: i64 = 0;
    let mut input_tokens: i64 = 0;
    let mut output_tokens: i64 = 0;
    let mut total_cost: f64 = 0.0;
    let mut agent_counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();

    for m in messages {
        if let Some(ref agent) = m.message.data.agent {
            *agent_counts.entry(agent.clone()).or_insert(0) += 1;
        }
        if let Some(ref token) = m.message.data.tokens {
            total_tokens += token.total.unwrap_or(0);
            input_tokens += token.input.unwrap_or(0);
            output_tokens += token.output.unwrap_or(0);
        }
        total_cost += m.message.data.cost.unwrap_or(0.0);
    }

    UsageAgg { total_tokens, input_tokens, output_tokens, total_cost, agent_counts }
}

fn cell_str(out: &mut String, label: &str, v1: &str, v2: &str) {
    writeln!(out, "| {} | {} | {} |", label, v1, v2).ok();
}

fn cell_usize(out: &mut String, label: &str, v1: usize, v2: usize) {
    writeln!(out, "| {} | {} | {} |", label, v1, v2).ok();
}

fn cell_i64(out: &mut String, label: &str, v1: i64, v2: i64) {
    writeln!(out, "| {} | {} | {} |", label, v1, v2).ok();
}

pub fn render_compare_json(
    s1: &Session,
    s2: &Session,
    mp1: &[MessageWithParts],
    mp2: &[MessageWithParts],
    _stats_only: bool,
) -> Result<String, anyhow::Error> {
    use serde::Serialize;

    let agg1 = aggregate_usage(mp1);
    let agg2 = aggregate_usage(mp2);

    #[derive(Serialize)]
    struct CompareOutput {
        session_a: CompareSessionView,
        session_b: CompareSessionView,
    }

    #[derive(Serialize)]
    struct CompareSessionView {
        id: String,
        title: String,
        model: Option<String>,
        message_count: usize,
        time_created: i64,
        directory: String,
        total_tokens: i64,
        input_tokens: i64,
        output_tokens: i64,
        total_cost: f64,
        summary_additions: Option<i64>,
        summary_deletions: Option<i64>,
        agent_counts: std::collections::BTreeMap<String, usize>,
    }

    let view = |s: &Session, agg: &UsageAgg| -> CompareSessionView {
        CompareSessionView {
            id: s.id.clone(),
            title: s.title.clone(),
            model: s.model.clone(),
            message_count: agg.agent_counts.values().sum(),
            time_created: s.time_created,
            directory: s.directory.clone(),
            total_tokens: agg.total_tokens,
            input_tokens: agg.input_tokens,
            output_tokens: agg.output_tokens,
            total_cost: agg.total_cost,
            summary_additions: s.summary_additions,
            summary_deletions: s.summary_deletions,
            agent_counts: agg.agent_counts.clone(),
        }
    };

    let output = CompareOutput {
        session_a: view(s1, &agg1),
        session_b: view(s2, &agg2),
    };

    Ok(serde_json::to_string_pretty(&output)?)
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
