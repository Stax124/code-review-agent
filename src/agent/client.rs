use std::sync::Arc;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{
    agent::response::{
        OpenRouterAPIResponse, OpenRouterAPIResponseChoice, OpenRouterAPIResponseMessage,
        OpenRouterAPIResponseUsage, StreamChunk, StreamEvent,
    },
    tools::AgentTool,
};

/// Represents all possible messages that are used to communicate between the agent and the client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AgentClientMessage {
    System(AgentClientMessageSystem),
    User(AgentClientMessageUser),
    Assistant(AgentClientMessageAssistant),
    Tool(AgentClientMessageTool),
}

/// Represents a system message sent to the agent, which may include instructions or context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentClientMessageSystem {
    pub role: String,
    pub content: String,
}

/// Represents a message sent by the user to the agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentClientMessageUser {
    pub role: String,
    pub content: String,
}

/// Represents a message sent by the agent to the user, which may include tool calls.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentClientMessageAssistant {
    pub role: String,
    pub content: Option<String>,
    pub reasoning: Option<String>,
    pub tool_calls: Option<Vec<AgentClientMessageToolCall>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentClientMessageToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub function: AgentClientMessageToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentClientMessageToolFunction {
    pub name: String,
    /// Needs to be parsed as a JSON object later, since it is already escaped as a string in the API response.
    pub arguments: String,
}

/// Represents the data returned by this client that is sent to the agent for processing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentClientMessageTool {
    pub role: String,
    pub tool_call_id: String,
    pub content: Option<String>,
}

/// Accumulated state while consuming a streaming response.
struct StreamAccumulator {
    role: String,
    content: String,
    reasoning: String,
    tool_calls: Vec<AgentClientMessageToolCall>,
    finish_reason: String,
    usage: Option<OpenRouterAPIResponseUsage>,
    chunk_id: String,
    chunk_model: String,
    chunk_provider: String,
}

impl Default for StreamAccumulator {
    fn default() -> Self {
        Self {
            role: String::from("assistant"),
            content: String::new(),
            reasoning: String::new(),
            tool_calls: Vec::new(),
            finish_reason: String::from("stop"),
            usage: None,
            chunk_id: String::new(),
            chunk_model: String::new(),
            chunk_provider: String::new(),
        }
    }
}

pub struct AgentClient {
    pub messages: Vec<AgentClientMessage>,
    pub temperature: f32,
    pub model: String,
    pub endpoint: String,
    pub http_client: reqwest::Client,
    pub available_tools: IndexMap<String, Arc<Box<dyn AgentTool>>>,
}

impl AgentClient {
    pub fn builder() -> AgentClientBuilder {
        AgentClientBuilder::new()
    }

    pub fn add_message(&mut self, message: AgentClientMessage) {
        self.messages.push(message);
    }

    pub fn build_tool_body(&self) -> Vec<serde_json::Value> {
        let tools: Vec<serde_json::Value> = self
            .available_tools
            .values()
            .map(|tool| tool.to_tool_schema())
            .collect();

        tools
    }

    pub fn build_request_body(&self) -> serde_json::Value {
        serde_json::json!({
            "model": self.model,
            "messages": self.messages,
            "temperature": self.temperature,
            "tools": self.build_tool_body(),
            "stream": true,
            "stream_options": { "include_usage": true }
        })
    }

    pub async fn process_tool_calls(
        &mut self,
        tool_calls: Vec<AgentClientMessageToolCall>,
    ) -> color_eyre::Result<Vec<String>> {
        tracing::debug!(
            "Parsed tools: {}",
            serde_json::to_string(&tool_calls).unwrap_or_default()
        );

        let mut to_display_to_user: Vec<String> = Vec::new();

        // Process each tool call and execute the corresponding tool if it exists in the available tools.
        for tool_call in tool_calls {
            if let Some(tool) = self.available_tools.get(&tool_call.function.name) {
                let tool = Arc::clone(tool);
                let parsed_arguments =
                    serde_json::from_str::<serde_json::Value>(&tool_call.function.arguments)
                        .unwrap_or_else(|_| {
                            tracing::error!(
                                "Failed to parse arguments for tool '{}'. Using empty object.",
                                tool_call.function.name
                            );
                            serde_json::json!({})
                        });

                match tool.execute(parsed_arguments).await {
                    Ok((result, display)) => {
                        tracing::debug!(
                            "Tool '{}' executed successfully. Result: {}",
                            tool_call.function.name,
                            serde_json::to_string(&result).unwrap_or_default()
                        );

                        to_display_to_user.push(display);

                        // Append it to the messages vector for future requests to the agent.
                        let tool_message = AgentClientMessage::Tool(AgentClientMessageTool {
                            role: "tool".to_string(),
                            tool_call_id: tool_call.id.clone(),
                            content: Some(result),
                        });
                        self.messages.push(tool_message);
                    }
                    Err(e) => {
                        tracing::warn!("Error executing tool '{}': {}", tool_call.function.name, e);
                        // Append an error message to the messages vector for future requests to the agent.
                        let error_message = AgentClientMessage::Tool(AgentClientMessageTool {
                            role: "tool".to_string(),
                            tool_call_id: tool_call.id.clone(),
                            content: Some(format!(
                                "Error executing tool '{}': {}",
                                tool_call.function.name, e
                            )),
                        });
                        to_display_to_user
                            .push(format!("{}: ERROR - {}", tool_call.function.name, e));
                        self.messages.push(error_message);
                    }
                }
            } else {
                tracing::warn!(
                    "Tool '{}' not found in available tools.",
                    tool_call.function.name
                );
                to_display_to_user.push(format!("{}: NOT FOUND", tool_call.function.name));
            }
        }

        Ok(to_display_to_user)
    }

    /// Start the communication with external API and process the streaming response
    pub async fn send(
        &mut self,
        mut on_event: impl FnMut(StreamEvent),
    ) -> color_eyre::Result<(OpenRouterAPIResponse, bool, Vec<String>)> {
        let request_body = self.build_request_body();

        let response = self
            .http_client
            .post(&self.endpoint)
            .json(&request_body)
            .send()
            .await?;

        // Errors before any tokens are sent arrive as a plain JSON body with a
        // non-2xx status code.
        if !response.status().is_success() {
            color_eyre::eyre::bail!(
                "Request failed with status: {}, Error: {}",
                response.status(),
                response.text().await?
            );
        }

        // Accumulators for the streamed response.
        let mut acc = StreamAccumulator::default();

        // SSE line buffer. OpenRouter sends `data: {json}\n\n` events and
        // occasional `: OPENROUTER PROCESSING` keep-alive comments.
        let mut buffer = String::new();
        let mut stream = response;

        while let Some(chunk) = stream.chunk().await? {
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Guard against unbounded buffer growth
            const MAX_LINE_LEN: usize = 1024 * 1024; // 1 MiB
            if buffer.len() > MAX_LINE_LEN && buffer.find('\n').is_none() {
                color_eyre::eyre::bail!(
                    "SSE line exceeded maximum length ({MAX_LINE_LEN} bytes) without a newline"
                );
            }

            // Process every complete line currently in the buffer.
            while let Some(newline_pos) = buffer.find('\n') {
                let line: String = buffer.drain(..=newline_pos).collect();
                let line = line.trim();

                // Skip empty lines (SSE event separators) and keep-alive
                // comments (lines starting with ':').
                if line.is_empty() || line.starts_with(':') {
                    continue;
                }

                // Every data event is prefixed with `data:`. Per the SSE spec
                // the colon is the delimiter and one optional leading space may
                // follow, so accept both `data: ` and `data:`.
                let Some(data) = line
                    .strip_prefix("data:")
                    .map(|d| d.strip_prefix(' ').unwrap_or(d))
                else {
                    continue;
                };

                // `[DONE]` marks the end of the stream.
                if data == "[DONE]" {
                    on_event(StreamEvent::Done);
                    return self.finalize_stream(acc).await;
                }

                let parsed: StreamChunk = serde_json::from_str(data).map_err(|e| {
                    color_eyre::eyre::eyre!("Failed to parse stream chunk: {e}. Raw: {data}")
                })?;

                // Capture metadata from the first chunk that provides it.
                if acc.chunk_id.is_empty()
                    && let Some(id) = &parsed.id
                {
                    acc.chunk_id = id.clone();
                }
                if acc.chunk_model.is_empty()
                    && let Some(m) = &parsed.model
                {
                    acc.chunk_model = m.clone();
                }
                if acc.chunk_provider.is_empty()
                    && let Some(p) = &parsed.provider
                {
                    acc.chunk_provider = p.clone();
                }

                // Mid-stream errors arrive as a chunk with a top-level `error`.
                if let Some(err) = &parsed.error {
                    color_eyre::eyre::bail!("Stream error: {}", err.message);
                }

                if let Some(u) = parsed.usage {
                    acc.usage = Some(u);
                }

                for choice in parsed.choices {
                    if let Some(r) = choice.delta.role {
                        acc.role = r;
                    }
                    // Reasoning
                    if let Some(r) = choice.delta.reasoning
                        && !r.is_empty()
                    {
                        acc.reasoning.push_str(&r);
                        on_event(StreamEvent::ReasoningDelta(r));
                    }
                    // Content
                    if let Some(c) = choice.delta.content
                        && !c.is_empty()
                    {
                        acc.content.push_str(&c);
                        on_event(StreamEvent::ContentDelta(c));
                    }
                    // Finish
                    if let Some(fr) = choice.finish_reason {
                        acc.finish_reason = fr;
                    }
                    // Tool calls
                    if let Some(tc_deltas) = choice.delta.tool_calls {
                        for tc in tc_deltas {
                            // Grow the accumulator to fit this tool call's index.
                            // Validate the index to prevent unbounded allocation
                            // from a malformed or adversarial API response.
                            const MAX_TOOL_CALLS: usize = 128;
                            let idx = tc.index as usize;
                            if idx >= MAX_TOOL_CALLS {
                                color_eyre::eyre::bail!(
                                    "Tool call index {} exceeds maximum ({})",
                                    idx,
                                    MAX_TOOL_CALLS
                                );
                            }
                            if idx >= acc.tool_calls.len() {
                                acc.tool_calls.resize_with(idx + 1, || {
                                    AgentClientMessageToolCall {
                                        id: String::new(),
                                        type_: "function".to_string(),
                                        function: AgentClientMessageToolFunction {
                                            name: String::new(),
                                            arguments: String::new(),
                                        },
                                    }
                                });
                            }
                            let slot = &mut acc.tool_calls[idx];
                            if let Some(id) = tc.id {
                                slot.id = id;
                            }
                            if let Some(t) = tc.type_ {
                                slot.type_ = t;
                            }
                            if let Some(func) = tc.function {
                                if let Some(name) = func.name {
                                    slot.function.name = name;
                                }
                                if let Some(args) = func.arguments {
                                    slot.function.arguments.push_str(&args);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Stream ended without an explicit `[DONE]` marker.
        on_event(StreamEvent::Done);
        self.finalize_stream(acc).await
    }

    /// Assemble the accumulated stream state into an assistant message and process any requested tool calls.
    async fn finalize_stream(
        &mut self,
        acc: StreamAccumulator,
    ) -> color_eyre::Result<(OpenRouterAPIResponse, bool, Vec<String>)> {
        let StreamAccumulator {
            role,
            content,
            reasoning,
            tool_calls,
            finish_reason,
            usage,
            chunk_id,
            chunk_model,
            chunk_provider,
        } = acc;

        let are_any_tools_requested = !tool_calls.is_empty();
        let content_opt = if content.is_empty() {
            None
        } else {
            Some(content)
        };
        let reasoning_opt = if reasoning.is_empty() {
            None
        } else {
            Some(reasoning)
        };
        let tool_calls_opt = if tool_calls.is_empty() {
            None
        } else {
            Some(tool_calls.clone())
        };

        let assistant_message = AgentClientMessageAssistant {
            role: role.clone(),
            content: content_opt.clone(),
            reasoning: reasoning_opt.clone(),
            tool_calls: tool_calls_opt.clone(),
        };

        self.messages
            .push(AgentClientMessage::Assistant(assistant_message));

        let to_display = if are_any_tools_requested {
            self.process_tool_calls(tool_calls).await?
        } else {
            Vec::new()
        };

        let api_response = OpenRouterAPIResponse {
            id: chunk_id,
            model: chunk_model,
            provider: chunk_provider,
            choices: vec![OpenRouterAPIResponseChoice {
                index: 0,
                finish_reason,
                message: OpenRouterAPIResponseMessage {
                    role,
                    content: content_opt,
                    reasoning: reasoning_opt,
                    refusal: None,
                    tool_calls: tool_calls_opt,
                },
            }],
            usage: usage.unwrap_or(OpenRouterAPIResponseUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
                cost: 0.0,
            }),
        };

        Ok((api_response, are_any_tools_requested, to_display))
    }
}

pub struct AgentClientBuilder {
    messages: Vec<AgentClientMessage>,
    temperature: f32,
    model: Option<String>,
    endpoint: Option<String>,
    api_key: Option<String>,
    available_tools: Vec<Box<dyn AgentTool>>,
}

impl AgentClientBuilder {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            temperature: 0.0,
            model: None,
            endpoint: None,
            api_key: None,
            available_tools: Vec::new(),
        }
    }

    pub fn with_tool(mut self, tool: Box<dyn AgentTool>) -> Self {
        self.available_tools.push(tool);
        self
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }

    pub fn with_system_message(mut self, content: &str) -> Self {
        self.messages
            .push(AgentClientMessage::System(AgentClientMessageSystem {
                role: "system".to_string(),
                content: content.to_string(),
            }));
        self
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = Some(model.to_string());
        self
    }

    pub fn with_endpoint(mut self, endpoint: &str) -> Self {
        self.endpoint = Some(endpoint.trim_end_matches('/').to_string()); // Remove trailing slash if present
        self
    }

    pub fn with_api_key(mut self, api_key: &str) -> Self {
        self.api_key = Some(api_key.to_string());
        self
    }

    pub fn build(self) -> color_eyre::Result<AgentClient> {
        if self.model.is_none() {
            color_eyre::eyre::bail!("Model must be specified before building the AgentClient.");
        }
        if self.api_key.is_none() {
            color_eyre::eyre::bail!("API key must be specified before building the AgentClient.");
        }
        if self.endpoint.is_none() {
            color_eyre::eyre::bail!("Endpoint must be specified before building the AgentClient.");
        }

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "Authorization",
            format!("Bearer {}", self.api_key.unwrap()).parse()?,
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .connect_timeout(std::time::Duration::from_secs(30))
            .read_timeout(std::time::Duration::from_secs(120))
            .build()?;

        Ok(AgentClient {
            messages: self.messages,
            temperature: self.temperature,
            model: self.model.unwrap_or_default(),
            endpoint: self.endpoint.unwrap_or_default(),
            http_client: client,
            available_tools: self
                .available_tools
                .into_iter()
                .map(|tool| (tool.name().to_string(), Arc::new(tool)))
                .collect(),
        })
    }
}
