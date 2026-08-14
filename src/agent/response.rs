use serde::{Deserialize, Serialize};

use crate::agent::client::AgentClientMessageToolCall;

#[derive(Debug, Deserialize, Serialize)]
pub struct OpenRouterAPIResponse {
    pub id: String,
    pub model: String,
    pub provider: String,
    pub choices: Vec<OpenRouterAPIResponseChoice>,
    pub usage: OpenRouterAPIResponseUsage,
}

/// Events emitted during streaming responses.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// A fragment of the model's reasoning
    ReasoningDelta(String),
    /// A fragment of the model's visible output
    ContentDelta(String),
    /// Emitted once after the stream has fully ended
    Done,
}

/// A single SSE chunk from the OpenRouter streaming API.
#[derive(Debug, Deserialize)]
pub struct StreamChunk {
    pub id: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    #[serde(default)]
    pub choices: Vec<StreamChoice>,
    pub usage: Option<OpenRouterAPIResponseUsage>,
    pub error: Option<StreamError>,
}

#[derive(Debug, Deserialize)]
pub struct StreamChoice {
    #[allow(dead_code)]
    pub index: u32,
    pub finish_reason: Option<String>,
    pub delta: StreamDelta,
}

#[derive(Debug, Deserialize)]
pub struct StreamDelta {
    pub role: Option<String>,
    pub content: Option<String>,
    pub reasoning: Option<String>,
    pub tool_calls: Option<Vec<StreamToolCall>>,
}

/// A tool-call delta. The first delta typically carries the tool `id` and function `name`
/// Subsequent deltas for the same index carry `arguments` fragments that must be concatenated.
#[derive(Debug, Deserialize)]
pub struct StreamToolCall {
    pub index: u32,
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub type_: Option<String>,
    pub function: Option<StreamToolFunction>,
}

#[derive(Debug, Deserialize)]
pub struct StreamToolFunction {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

/// A mid-stream error.
#[derive(Debug, Deserialize)]
pub struct StreamError {
    // Kept as a raw JSON value (number or string depending on provider)
    #[allow(dead_code)]
    pub code: serde_json::Value,
    pub message: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OpenRouterAPIResponseChoice {
    pub index: u32,
    pub finish_reason: String,
    pub message: OpenRouterAPIResponseMessage,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OpenRouterAPIResponseMessage {
    pub role: String,
    pub content: Option<String>,
    pub reasoning: Option<String>,
    pub refusal: Option<String>,
    pub tool_calls: Option<Vec<AgentClientMessageToolCall>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OpenRouterAPIResponseUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    pub cost: f64,
}
