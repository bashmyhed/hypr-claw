use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: u64,
    pub account_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextData {
    pub session_id: String,
    pub system_state: serde_json::Value,
    pub facts: Vec<String>,
    pub recent_history: Vec<HistoryEntry>,
    pub long_term_summary: String,
    pub active_tasks: Vec<TaskState>,
    pub tool_stats: ToolStats,
    pub last_known_environment: EnvironmentData,
    pub token_usage: TokenUsage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_tokens: Option<OAuthTokens>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp: i64,
    pub role: String,
    pub content: String,
    pub token_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    pub id: String,
    pub description: String,
    pub status: String,
    pub progress: f32,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolStats {
    pub total_calls: u64,
    pub by_tool: std::collections::HashMap<String, u64>,
    pub failures: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EnvironmentData {
    pub workspace: String,
    pub last_update: i64,
    pub system_snapshot: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub total_input: usize,
    pub total_output: usize,
    pub by_session: usize,
}

impl Default for ContextData {
    fn default() -> Self {
        Self {
            session_id: String::new(),
            system_state: serde_json::json!({}),
            facts: Vec::new(),
            recent_history: Vec::new(),
            long_term_summary: String::new(),
            active_tasks: Vec::new(),
            tool_stats: ToolStats::default(),
            last_known_environment: EnvironmentData::default(),
            token_usage: TokenUsage::default(),
            oauth_tokens: None,
        }
    }
}
