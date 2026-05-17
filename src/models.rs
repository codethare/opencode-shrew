use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct Session {
    pub id: String,
    pub project_id: String,
    pub slug: String,
    pub directory: String,
    pub title: String,
    pub time_created: i64,
    pub time_updated: i64,
    pub summary_additions: Option<i64>,
    pub summary_deletions: Option<i64>,
    pub summary_files: Option<i64>,
    pub model: Option<String>,
    pub msg_count: i64,
    #[serde(default)]
    pub total_cost: f64,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub id: String,
    #[allow(dead_code)]
    pub session_id: String,
    pub time_created: i64,
    pub data: MessageData,
}

#[derive(Debug, Clone, Serialize)]
pub struct Part {
    pub id: String,
    pub message_id: String,
    pub data: PartData,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MessageData {
    pub role: String,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub model: Option<ModelInfo>,
    #[serde(default)]
    pub tokens: Option<TokenUsage>,
    #[serde(default)]
    pub cost: Option<f64>,
    #[allow(dead_code)]
    #[serde(default)]
    pub mode: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    pub parent_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelInfo {
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TokenUsage {
    #[serde(default)]
    pub total: Option<i64>,
    #[serde(default)]
    pub input: Option<i64>,
    #[serde(default)]
    pub output: Option<i64>,
    #[serde(default)]
    pub reasoning: Option<i64>,
    #[serde(default)]
    pub cache: Option<CacheUsage>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CacheUsage {
    #[serde(default)]
    pub write: Option<i64>,
    #[serde(default)]
    pub read: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PartData {
    pub r#type: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub call_id: Option<String>,
    #[serde(default)]
    pub state: Option<ToolState>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub time: Option<TimeRange>,
    #[serde(default)]
    pub done: Option<bool>,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolState {
    pub status: String,
    #[serde(default)]
    pub input: Option<serde_json::Value>,
    #[serde(default)]
    pub output: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TimeRange {
    #[serde(default)]
    pub start: Option<i64>,
    #[serde(default)]
    pub end: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct MessageWithParts {
    pub message: Message,
    pub parts: Vec<Part>,
}

impl MessageWithParts {
    pub fn text_body(&self) -> String {
        self.parts
            .iter()
            .filter(|p| p.data.r#type == "text" || p.data.r#type == "reasoning")
            .filter_map(|p| p.data.text.as_deref())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn reasoning_body(&self) -> Option<String> {
        let texts: Vec<&str> = self.parts
            .iter()
            .filter(|p| p.data.r#type == "reasoning")
            .filter_map(|p| p.data.text.as_deref())
            .filter(|t| !t.is_empty())
            .collect();
        if texts.is_empty() { None } else { Some(texts.join("\n")) }
    }

    pub fn tool_calls(&self) -> Vec<&Part> {
        self.parts.iter().filter(|p| p.data.r#type == "tool").collect()
    }

    pub fn step_finish(&self) -> Option<&Part> {
        self.parts.iter().find(|p| p.data.r#type == "step-finish")
    }
}

// ── Search results ──

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub session_id: String,
    pub session_title: String,
    pub time_created: i64,
    pub message_id: String,
    pub msg_time_created: i64,
    pub snippet: String,
}

// ── Session stats (aggregated) ──

#[derive(Debug, Clone, Serialize)]
pub struct SessionStats {
    pub session_id: String,
    pub session_title: String,
    pub total_messages: i64,
    pub user_messages: i64,
    pub assistant_messages: i64,
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub cache_write: i64,
    pub cache_read: i64,
    pub total_cost: f64,
    pub agent_breakdown: Vec<AgentBreakdown>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentBreakdown {
    pub agent: String,
    pub message_count: i64,
    pub total_tokens: i64,
    pub total_cost: f64,
}

// ── Top sessions entry ──

#[derive(Debug, Clone, Serialize)]
pub struct TopSessionEntry {
    pub id: String,
    pub title: String,
    pub directory: String,
    pub time_created: i64,
    pub msg_count: i64,
    pub total_cost: f64,
    pub total_tokens: i64,
    pub total_input: i64,
    pub total_output: i64,
}

// ── Diff entry ──

#[derive(Debug, Clone, Deserialize)]
pub struct DiffEntry {
    pub file: String,
    #[serde(default)]
    pub patch: Option<String>,
    #[serde(default)]
    pub additions: Option<i64>,
    #[serde(default)]
    pub deletions: Option<i64>,
    pub status: Option<String>,
}

// ── Project groups ──

#[derive(Debug, Clone, Serialize)]
pub struct ReportSummary {
    pub total_sessions: i64,
    pub total_messages: i64,
    pub total_tokens: i64,
    pub total_cost: f64,
    pub period_start: String,
    pub period_end: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyTrend {
    pub date: String,
    pub sessions: i64,
    pub messages: i64,
    pub tokens: i64,
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelBreakdown {
    pub model: String,
    pub message_count: i64,
    pub total_tokens: i64,
    pub total_cost: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectGroup {
    pub directory: String,
    pub session_count: i64,
    pub total_messages: i64,
    pub last_active: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoTagRule {
    pub id: String,
    pub tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_contains: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir_contains: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_messages: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_messages: Option<i64>,
}
