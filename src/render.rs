use chrono::DateTime;
use std::sync::OnceLock;

use regex::Regex;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::as_24_bit_terminal_escaped;
use unicode_width::UnicodeWidthStr;

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
            let project = s.directory.rsplit('/').next().unwrap_or(&s.directory);
            let cost_str = if s.total_cost > 0.0 {
                format!("${:.2}", s.total_cost)
            } else {
                String::new()
            };
            format!("{}\t{}\t{}\t{}\t{}\t{}\t{}", s.id, created_ts, s.msg_count, model, cost_str, project, s.title)
        })
        .collect()
}

/// Render a full session with its messages as markdown
pub fn render_session_detail(session: &Session, messages: &[MessageWithParts], show_tools: bool) -> String {
    let mut out = render_session_header(session);
    for msg_with_parts in messages {
        render_message(&mut out, msg_with_parts, show_tools);
    }
    out
}

pub fn render_session_header(session: &Session) -> String {
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
    out
}

pub fn render_message_batch(messages: &[MessageWithParts], show_tools: bool) -> String {
    let mut out = String::new();
    for msg in messages {
        render_message(&mut out, msg, show_tools);
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

/// Render a session preview for interactive selection (compact info card)
pub fn render_session_preview(s: &Session) -> String {
    let created = ts_to_compact(s.time_created);
    let model = s.model.as_deref().unwrap_or("unknown");
    let cost = format!("${:.4}", s.total_cost);
    let project = s.directory.rsplit('/').next().unwrap_or(&s.directory);
    let diff = match (s.summary_additions, s.summary_deletions) {
        (Some(a), Some(d)) if a > 0 || d > 0 => format!("(+{}/{})", a, d),
        _ => String::new(),
    };
    format!(
        "\
────────────────────────────────────────\n\
  Title:    {title}\n\
  Session:  {id}\n\
  Model:    {model}\n\
  Project:  {project}\n\
  Messages: {msgs}\n\
  Cost:     {cost}\n\
  Created:  {created} {diff}\n\
────────────────────────────────────────",
        title = s.title,
        id = s.id,
        model = model,
        project = project,
        msgs = s.msg_count,
        cost = cost,
        created = created,
        diff = diff,
    )
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

/// Render session context summary (metadata + tags + stats + related)
pub fn render_session_context(session: &Session, stats: &SessionStats, tags: &[String], note: Option<&str>, related: &[Session]) -> String {
    let mut out = String::new();
    let created_ts = ts_to_iso_short(session.time_created);
    let model = session.model.as_deref().unwrap_or("unknown");

    out.push_str(&format!("# Context: {}\n\n", session.title));
    out.push_str("---\n\n");
    out.push_str(&format!("**Session**: `{}`  \n", session.id));
    out.push_str(&format!("**Model**: `{}`  \n", model));
    out.push_str(&format!("**Directory**: `{}`  \n", session.directory));
    out.push_str(&format!("**Created**: {}  \n", created_ts));
    out.push_str(&format!("**Messages**: {}  \n", session.msg_count));

    if session.summary_additions.unwrap_or(0) > 0 || session.summary_deletions.unwrap_or(0) > 0 {
        out.push_str(&format!("**Files changed**: {} (+{}/-{})  \n",
            session.summary_files.unwrap_or(0),
            session.summary_additions.unwrap_or(0),
            session.summary_deletions.unwrap_or(0)));
    }

    // Tags
    if !tags.is_empty() {
        out.push('\n');
        out.push_str(&format!("**Tags**: {}  \n", tags.join(", ")));
    }

    // Annotation
    if let Some(n) = note {
        out.push('\n');
        out.push_str("---\n\n");
        out.push_str("## Annotation\n\n");
        out.push_str(n);
        out.push('\n');
    }

    // Stats summary
    out.push('\n');
    out.push_str("---\n\n");
    out.push_str("## Token Usage\n\n");
    out.push_str(&format!("| Metric | Value |\n|---|---|\n"));
    out.push_str(&format!("| Total Tokens | {} |\n", stats.total_tokens));
    out.push_str(&format!("| Input / Output | {} / {} |\n", stats.input_tokens, stats.output_tokens));
    out.push_str(&format!("| Reasoning | {} |\n", stats.reasoning_tokens));
    out.push_str(&format!("| Total Cost | ${:.6} |\n", stats.total_cost));

    if !stats.agent_breakdown.is_empty() {
        out.push('\n');
        out.push_str("## Agents\n\n");
        out.push_str("| Agent | Messages | Tokens | Cost |\n|---|---|---|---|\n");
        for a in &stats.agent_breakdown {
            out.push_str(&format!("| {} | {} | {} | ${:.6} |\n",
                a.agent, a.message_count, a.total_tokens, a.total_cost));
        }
    }

    // Related sessions
    if !related.is_empty() {
        out.push('\n');
        out.push_str("---\n\n");
        out.push_str(&format!("## Related Sessions (same project)\n\n"));
        out.push_str("| Session | Title | Messages | Model | Created |\n|---|---|---|---|---|\n");
        for r in related {
            let r_ts = ts_to_iso_short(r.time_created);
            let r_model = r.model.as_deref().unwrap_or("—");
            let short_id = if r.id.chars().count() > 12 {
                format!("{}…", r.id.chars().take(12).collect::<String>())
            } else {
                r.id.clone()
            };
            out.push_str(&format!("| `{}` | {} | {} | {} | {} |\n",
                short_id, r.title, r.msg_count, r_model, r_ts));
        }
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
            if body.chars().count() > 80 {
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
            if body.chars().count() > 80 {
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
    let nsecs = (ts.rem_euclid(1000) * 1_000_000) as u32;
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
    let nsecs = (ts.rem_euclid(1000) * 1_000_000) as u32;
    match DateTime::from_timestamp(secs, nsecs) {
        Some(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
        None => format!("(invalid timestamp {})", ts),
    }
}

pub(crate) fn ts_to_compact(ts: i64) -> String {
    let secs = ts / 1000;
    let nsecs = (ts.rem_euclid(1000) * 1_000_000) as u32;
    match DateTime::from_timestamp(secs, nsecs) {
        Some(dt) => dt.format("%m-%d %H:%M").to_string(),
        None => String::new(),
    }
}

fn ts_to_time(ts: i64) -> String {
    let secs = ts / 1000;
    let nsecs = (ts.rem_euclid(1000) * 1_000_000) as u32;
    match DateTime::from_timestamp(secs, nsecs) {
        Some(dt) => dt.format("%H:%M:%S").to_string(),
        None => String::new(),
    }
}

struct HighlightResources {
    ss: SyntaxSet,
    ts: ThemeSet,
}

fn highlight_resources() -> &'static HighlightResources {
    static RES: OnceLock<HighlightResources> = OnceLock::new();
    RES.get_or_init(|| HighlightResources {
        ss: SyntaxSet::load_defaults_newlines(),
        ts: ThemeSet::load_defaults(),
    })
}

fn code_block_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?ms)^```(\w*)\n(.*?)^```\s*$").unwrap())
}

/// Apply ANSI terminal styling to markdown session output for TUI-like readability.
/// Highlights headers, metadata, inline elements — skips code fences for syntect.
pub fn apply_terminal_styles(text: &str) -> String {
    use regex::Regex;
    use std::sync::OnceLock;

    const BOLD: &str = "\x1b[1m";
    const DIM: &str = "\x1b[2m";
    const BRIGHT_GREEN: &str = "\x1b[92m";
    const YELLOW: &str = "\x1b[33m";
    const RESET: &str = "\x1b[0m";

    /// Line-level: # Title → bold
    fn h1_re() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"(?m)^# (.+)$").unwrap())
    }
    /// Line-level: ## Header → bold bright green
    fn h2_re() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"(?m)^## (.+)$").unwrap())
    }
    /// Line-level: --- → dim
    fn sep_re() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"(?m)^-{3,}\s*$").unwrap())
    }
    /// Inline: `code` → cyan (strip backticks)
    fn bt_re() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"`([^`]+)`").unwrap())
    }
    /// Inline: **bold** → bold (strip **)
    fn bold_re() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"\*\*([^*]+)\*\*").unwrap())
    }
    /// Inline: (+N/-M) → green +N / red -M
    fn diff_re() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"\((\+)(\d+)/-(\d+)\)").unwrap())
    }
    /// Inline: $0.0000 → yellow
    fn cost_re() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"\$[0-9]+\.[0-9]+").unwrap())
    }
    /// Inline: N tokens → magenta
    fn tok_re() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"\b[0-9]+ tokens\b").unwrap())
    }
    /// Line-level: > ⏹ Finished ... → bold yellow
    fn fin_re() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"(?m)^> ⏹ .+$").unwrap())
    }

    let s = h1_re().replace_all(text, |caps: &regex::Captures| {
        format!("{BOLD}{}{RESET}", &caps[1])
    });
    let s = h2_re().replace_all(&s, |caps: &regex::Captures| {
        format!("{BRIGHT_GREEN}{BOLD}{}{RESET}", &caps[1])
    });
    let s = sep_re().replace_all(&s, |caps: &regex::Captures| {
        format!("{DIM}{}{RESET}", &caps[0])
    });
    let s = fin_re().replace_all(&s, |caps: &regex::Captures| {
        format!("{BOLD}{YELLOW}{}{RESET}", &caps[0])
    });

    // Inline styles (inside non-code regions)
    fn style_inline(s: &str) -> String {
        const CYAN: &str = "\x1b[96m";
        const BOLD: &str = "\x1b[1m";
        const BOLD_OFF: &str = "\x1b[22m";
        const GREEN: &str = "\x1b[32m";
        const RED: &str = "\x1b[31m";
        const YELLOW: &str = "\x1b[33m";
        const MAGENTA: &str = "\x1b[35m";
        const RESET: &str = "\x1b[0m";

        let s = bt_re().replace_all(s, |caps: &regex::Captures| {
            format!("{CYAN}{}{RESET}", &caps[1])
        });
        let s = bold_re().replace_all(&s, |caps: &regex::Captures| {
            format!("{BOLD}{}{BOLD_OFF}", &caps[1])
        });
        let s = diff_re().replace_all(&s, |caps: &regex::Captures| {
            format!("({GREEN}+{}{RESET}/{RED}-{}{RESET})", &caps[2], &caps[3])
        });
        let s = cost_re().replace_all(&s, |caps: &regex::Captures| {
            format!("{YELLOW}{}{RESET}", &caps[0])
        });
        let s = tok_re().replace_all(&s, |caps: &regex::Captures| {
            format!("{MAGENTA}{}{RESET}", &caps[0])
        });
        s.into_owned()
    }

    let cb_re = code_block_re();
    let mut result = String::with_capacity(s.len() + 256);
    let mut last = 0;
    for cap in cb_re.captures_iter(&s) {
        let m = cap.get(0).unwrap();
        result.push_str(&style_inline(&s[last..m.start()]));
        result.push_str(m.as_str());
        last = m.end();
    }
    result.push_str(&style_inline(&s[last..]));
    result
}

/// Apply ANSI syntax highlighting to fenced code blocks (```lang ... ```) in text.
/// Only affects terminal output — no highlighting for JSON/markdown export.
pub fn highlight_code_blocks(text: &str) -> String {
    let re = code_block_re();
    let res = highlight_resources();
    let theme = &res.ts.themes["base16-ocean.dark"];

    let mut last_end = 0;
    let mut result = String::new();

    for cap in re.captures_iter(text) {
        let m = cap.get(0).unwrap();
        result.push_str(&text[last_end..m.start()]);

        let lang = cap.get(1).map_or("", |m| m.as_str());
        let code = cap.get(2).map_or("", |m| m.as_str());

        let syntax = res
            .ss
            .find_syntax_by_token(lang)
            .unwrap_or_else(|| res.ss.find_syntax_plain_text());

        let mut highlighter = HighlightLines::new(syntax, theme);

        for line in code.lines() {
            if let Ok(ranges) = highlighter.highlight_line(line, &res.ss) {
                result.push_str(&as_24_bit_terminal_escaped(&ranges, false));
            } else {
                result.push_str(line);
            }
            result.push('\n');
        }

        last_end = m.end();
    }

    result.push_str(&text[last_end..]);
    result
}

/// Strip ANSI escape codes from text (e.g. from subprocess output).
pub fn strip_ansi(text: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").unwrap());
    re.replace_all(text, "").into_owned()
}

/// Compute the visible terminal width of a string after stripping ANSI codes.
/// Uses [`UnicodeWidthStr::width`] so that CJK characters (width=2) and
/// combining characters (width=0) are handled correctly.
pub fn visible_width(text: &str) -> usize {
    UnicodeWidthStr::width(strip_ansi(text).as_str())
}

// ── Markdown → ANSI renderer ──────────────────────────────────────────────

use pulldown_cmark as pd;

/// Render markdown text to ANSI-terminal-formatted output.
///
/// Parses GFM markdown via `pulldown-cmark` and produces a richly formatted
/// terminal string with syntax-highlighted code blocks, styled tables, lists,
/// links, blockquotes, etc. Replaces the previous `apply_terminal_styles()` +
/// `highlight_code_blocks()` two-step pipeline.
fn preprocess_markdown(text: &str) -> String {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?s)<details>\n?<summary>(.*?)</summary>\s*(.*?)</details>")
            .expect("invalid details regex")
    });
    re.replace_all(text, |caps: &regex::Captures| {
        format!("**▼ {}**\n\n{}", &caps[1], &caps[2])
    })
    .into_owned()
}

/// Render markdown text to ANSI-terminal-formatted output.
///
/// Parses GFM markdown via `pulldown-cmark` and produces a richly formatted
/// terminal string with syntax-highlighted code blocks, styled tables, lists,
/// links, blockquotes, etc. Replaces the previous `apply_terminal_styles()` +
/// `highlight_code_blocks()` two-step pipeline.
pub fn render_markdown(text: &str) -> String {
    let cleaned = preprocess_markdown(text);
    let parser = pd::Parser::new_ext(&cleaned, pd::Options::all());
    let mut r = MdRenderer::new();
    for event in parser {
        match event {
            pd::Event::Start(tag) => r.start(&tag),
            pd::Event::End(tag) => r.end(&tag),
            pd::Event::Text(t) => r.text(&t),
            pd::Event::Code(t) => r.code(&t),
            pd::Event::SoftBreak | pd::Event::HardBreak => r.out("\n"),
            pd::Event::Rule => r.out(&"\x1b[2m────────────────────────────────────────────────────────────\x1b[0m\n"),
            pd::Event::TaskListMarker(checked) => {
                r.out(if checked { "\x1b[92m☑\x1b[0m " } else { "\x1b[2m☐\x1b[0m " });
            }
            // If any inline HTML slips through, render it dimmed
            pd::Event::Html(html) => {
                // Strip tags for cleaner display, render dimmed
                let stripped = html.replace('<', "").replace('>', "");
                if !stripped.trim().is_empty() {
                    r.out(&format!("\x1b[2m{}\x1b[0m", stripped));
                }
            }
            pd::Event::InlineHtml(html) => {
                if !html.trim().is_empty() {
                    r.out(&format!("\x1b[2m{}\x1b[0m", html));
                }
            }
            _ => {}
        }
    }
    r.finish()
}

struct MdRenderer {
    output: String,
    // saved outputs for buffering (blockquotes, table cells)
    saved: Vec<String>,
    // heading
    heading_level: u32,
    // list
    list_ordered: Vec<bool>,
    list_indexes: Vec<u64>,
    // code block
    code_buf: String,
    code_lang: String,
    // table
    tbl_aligns: Vec<pd::Alignment>,
    tbl_rows: Vec<Vec<String>>,
    tbl_row: Vec<String>,
    tbl_cell: String,
    tbl_in_head: bool,
    // link footnotes
    link_links: Vec<(String, String)>, // (text, url)
    // blockquote
    quote_depth: usize,
}

impl MdRenderer {
    fn new() -> Self {
        Self {
            output: String::new(),
            saved: Vec::new(),
            heading_level: 0,
            list_ordered: Vec::new(),
            list_indexes: Vec::new(),
            code_buf: String::new(),
            code_lang: String::new(),
            tbl_aligns: Vec::new(),
            tbl_rows: Vec::new(),
            tbl_row: Vec::new(),
            tbl_cell: String::new(),
            tbl_in_head: false,
            link_links: Vec::new(),
            quote_depth: 0,
        }
    }

    fn out(&mut self, s: &str) {
        self.output.push_str(s);
    }

    fn start(&mut self, tag: &pd::Tag) {
        match tag {
            pd::Tag::Heading { level, .. } => {
                self.heading_level = *level as u32;
                match *level as u32 {
                    1 => self.out("\x1b[1m"),
                    2 => self.out("\x1b[92m\x1b[1m"),
                    _ => self.out("\x1b[94m\x1b[1m"),
                }
            }
            pd::Tag::Paragraph => {
                // If inside a blockquote, paragraphs don't get extra spacing
            }
            pd::Tag::Emphasis => self.out("\x1b[3m"),
            pd::Tag::Strong => self.out("\x1b[1m"),
            pd::Tag::Strikethrough => self.out("\x1b[9m"),
            pd::Tag::Link { dest_url, .. } => {
                self.link_links.push((String::new(), dest_url.to_string()));
                self.out("\x1b[4m\x1b[34m");
            }
            pd::Tag::Image { dest_url, .. } => {
                self.out(&format!("\x1b[2m[{}]\x1b[0m", dest_url));
            }
            pd::Tag::List(opt) => {
                self.list_ordered.push(opt.is_some());
                self.list_indexes.push(opt.unwrap_or(1));
            }
            pd::Tag::Item => {
                if let Some(&ordered) = self.list_ordered.last() {
                    let level = self.list_ordered.len();
                    let indent = "  ".repeat(level - 1);
                    if ordered {
                        let n = {
                            let idx = self.list_indexes.last_mut().unwrap();
                            let n = *idx;
                            *idx += 1;
                            n
                        };
                        self.out(&format!("{indent}{n}.\x1b[0m "));
                    } else {
                        self.out(&format!("{indent}\x1b[37m•\x1b[0m "));
                    }
                }
            }
            pd::Tag::CodeBlock(kind) => {
                self.code_lang = match kind {
                    pd::CodeBlockKind::Fenced(info) => info.to_string(),
                    pd::CodeBlockKind::Indented => String::new(),
                };
                self.code_buf.clear();
            }
            pd::Tag::Table(aligns) => {
                self.tbl_aligns = aligns.clone();
                self.tbl_rows.clear();
                self.tbl_in_head = false;
            }
            pd::Tag::TableHead => {
                self.tbl_in_head = true;
            }
            pd::Tag::TableRow => {
                self.tbl_row.clear();
            }
            pd::Tag::TableCell => {
                self.tbl_cell.clear();
                let prev = std::mem::take(&mut self.output);
                self.saved.push(prev);
            }
            pd::Tag::BlockQuote(_) => {
                let prev = std::mem::take(&mut self.output);
                self.saved.push(prev);
                self.quote_depth += 1;
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: &pd::TagEnd) {
        match tag {
            pd::TagEnd::Heading(_) => {
                self.out("\x1b[0m\n");
                self.heading_level = 0;
            }
            pd::TagEnd::Paragraph => {
                // In list items, paragraphs are inline; in blockquotes,
                // spacing is handled by quote prefix logic
                let in_list = !self.list_ordered.is_empty();
                let in_quote = self.quote_depth > 0;
                if !in_list && !in_quote {
                    self.out("\n");
                }
            }
            pd::TagEnd::Emphasis => self.out("\x1b[23m"),
            pd::TagEnd::Strong => self.out("\x1b[22m"),
            pd::TagEnd::Strikethrough => self.out("\x1b[29m"),
            pd::TagEnd::Link => {
                let n = self.link_links.len();
                self.out(&format!("\x1b[0m\x1b[2m[{}]\x1b[0m", n));
            }
            pd::TagEnd::Image => {
                // Image alt text children were not rendered
            }
            pd::TagEnd::List(_tight) => {
                self.list_ordered.pop();
                self.list_indexes.pop();
                if self.list_ordered.is_empty() {
                    self.out("\n");
                }
            }
            pd::TagEnd::Item => {
                self.out("\n");
            }
            pd::TagEnd::CodeBlock => {
                // Highlight the collected code buffer
                let highlighted = highlight_snippet(&self.code_buf, &self.code_lang);
                self.out(&highlighted);
                self.out("\n");
            }
            pd::TagEnd::Table => {
                self.render_table();
            }
            pd::TagEnd::TableHead => {
                self.tbl_in_head = false;
            }
            pd::TagEnd::TableRow => {
                let row = std::mem::take(&mut self.tbl_row);
                self.tbl_rows.push(row);
            }
            pd::TagEnd::TableCell => {
                let cell = std::mem::replace(&mut self.output, self.saved.pop().unwrap());
                self.tbl_row.push(cell);
            }
            pd::TagEnd::BlockQuote(_) => {
                let content = std::mem::replace(&mut self.output, self.saved.pop().unwrap());
                for line in content.lines() {
                    self.out(&format!("\x1b[2m│\x1b[0m {}\n", line));
                }
                self.quote_depth -= 1;
            }
            _ => {}
        }
    }

    fn text(&mut self, t: &str) {
        if !self.code_buf.is_empty() || !self.code_lang.is_empty() {
            // In a code block — collect text for later highlighting
            self.code_buf.push_str(t);
            return;
        }
        let styled = style_data_text(t);
        self.out(&styled);
    }

    fn code(&mut self, t: &str) {
        self.out(&format!("\x1b[96m{}\x1b[0m", t));
    }

    fn finish(&mut self) -> String {
        // Append link footnotes
        for (i, &(_, ref url)) in self.link_links.iter().enumerate() {
            self.output.push_str(&format!("\x1b[2m [{}]: {}\x1b[0m\n", i + 1, url));
        }
        std::mem::take(&mut self.output)
    }

    fn render_table(&mut self) {
        let rows = std::mem::take(&mut self.tbl_rows);
        if rows.is_empty() {
            return;
        }
        let ncols = self.tbl_aligns.len().max(
            rows.iter().map(|r| r.len()).max().unwrap_or(0),
        );
        if ncols == 0 {
            return;
        }

        // Pad all rows to ncols
        let mut rows = rows;
        for row in &mut rows {
            while row.len() < ncols {
                row.push(String::new());
            }
        }

        let mut widths = vec![3usize; ncols];
        for row in &rows {
            for (i, cell) in row.iter().enumerate() {
                let w = visible_width(cell);
                if w > widths[i] {
                    widths[i] = w;
                }
            }
        }
        for w in &mut widths {
            *w = (*w).max(3);
        }

        // Top border
        self.top_border(&widths);
        self.out("\n");

        if !rows.is_empty() {
            self.data_row(&rows[0], &widths);
            self.sep_border(&widths);
            self.out("\n");
        }

        for row in &rows[1..] {
            self.data_row(row, &widths);
        }

        // Bottom border
        self.bottom_border(&widths);
        self.out("\n");
    }

    fn top_border(&mut self, widths: &[usize]) {
        self.out("┌");
        for (i, w) in widths.iter().enumerate() {
            self.out(&"─".repeat(w + 2));
            if i < widths.len() - 1 {
                self.out("┬");
            }
        }
        self.out("┐");
    }

    fn sep_border(&mut self, widths: &[usize]) {
        self.out("├");
        for (i, w) in widths.iter().enumerate() {
            self.out(&"─".repeat(w + 2));
            if i < widths.len() - 1 {
                self.out("┼");
            }
        }
        self.out("┤\n");
    }

    fn bottom_border(&mut self, widths: &[usize]) {
        self.out("└");
        for (i, w) in widths.iter().enumerate() {
            self.out(&"─".repeat(w + 2));
            if i < widths.len() - 1 {
                self.out("┴");
            }
        }
        self.out("┘\n");
    }

    fn data_row(&mut self, row: &[String], widths: &[usize]) {
        self.out("│");
        for (i, cell) in row.iter().enumerate() {
            let w = widths.get(i).copied().unwrap_or(3);
            self.out(" ");
            let visible_len = visible_width(cell);
            self.out(cell);
            if visible_len < w {
                self.out(&" ".repeat(w - visible_len));
            }
            self.out(" │");
        }
        self.out("\n");
    }
}

/// Apply data-specific ANSI styling to plain inline text (cost, tokens, diffs, finished markers).
/// Only called for `Event::Text` — never applied inside code blocks or inline code.
fn style_data_text(text: &str) -> String {
    use regex::Regex;
    use std::sync::OnceLock;

    const GREEN: &str = "\x1b[32m";
    const RED: &str = "\x1b[31m";
    const YELLOW: &str = "\x1b[33m";
    const MAGENTA: &str = "\x1b[35m";
    const BOLD: &str = "\x1b[1m";
    const RESET: &str = "\x1b[0m";

    fn diff_re() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"\((\+)(\d+)/-(\d+)\)").unwrap())
    }
    fn cost_re() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"\$[0-9]+\.[0-9]+").unwrap())
    }
    fn tok_re() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"\b[0-9]+ tokens\b").unwrap())
    }
    fn fin_re() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"(?m)^> ⏹ .+$").unwrap())
    }

    let s = fin_re().replace_all(text, |caps: &regex::Captures| {
        format!("{BOLD}{YELLOW}{}{RESET}", &caps[0])
    });
    let s = diff_re().replace_all(&s, |caps: &regex::Captures| {
        format!("({GREEN}+{}{RESET}/{RED}-{}{RESET})", &caps[2], &caps[3])
    });
    let s = cost_re().replace_all(&s, |caps: &regex::Captures| {
        format!("{YELLOW}{}{RESET}", &caps[0])
    });
    let s = tok_re().replace_all(&s, |caps: &regex::Captures| {
        format!("{MAGENTA}{}{RESET}", &caps[0])
    });
    s.into_owned()
}

/// Highlight a raw code snippet with syntect, given a language hint.
fn highlight_snippet(code: &str, lang: &str) -> String {
    const HIGHLIGHT_MAX_LINES: usize = 100;
    let line_count = code.lines().count();
    if line_count > HIGHLIGHT_MAX_LINES {
        return format!("\x1b[1m\x1b[38;5;245m┌─ {lang} ({} lines, highlighting skipped)\x1b[0m\n{code}\n\x1b[1m\x1b[38;5;245m└─\x1b[0m",
            line_count);
    }

    let res = highlight_resources();
    let theme = &res.ts.themes["base16-ocean.dark"];
    let syntax = res
        .ss
        .find_syntax_by_token(lang)
        .unwrap_or_else(|| res.ss.find_syntax_plain_text());
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut result = String::new();
    for line in code.lines() {
        if let Ok(ranges) = highlighter.highlight_line(line, &res.ss) {
            result.push_str(&as_24_bit_terminal_escaped(&ranges, false));
        } else {
            result.push_str(line);
        }
        result.push('\n');
    }
    result
}
