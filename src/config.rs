use clap::Parser;
use reqwest::Url;

/// Configuration for the code review agent.
///
/// Values can be set via CLI flags or `CODE_REVIEW_AGENT_*` environment variables.
/// CLI flags take precedence over env vars, which take precedence over defaults.
#[derive(Parser)]
#[command(version, about)]
pub struct Configuration {
    /// API key sent in the `Authorization` header
    #[arg(
        long,
        env = "CODE_REVIEW_AGENT_API_KEY",
        hide_env_values = true,
        help_heading = "General"
    )]
    pub api_key: String,

    /// OpenAI API compatible endpoint, e.g. `https://openrouter.ai/api/v1/chat/completions`
    #[arg(
        long,
        env = "CODE_REVIEW_AGENT_API_ENDPOINT",
        default_value = "https://openrouter.ai/api/v1/chat/completions",
        help_heading = "General"
    )]
    pub api_endpoint: String,

    /// Model to use for code review, e.g. `z-ai/glm-5.2`
    #[arg(
        long,
        env = "CODE_REVIEW_AGENT_MODEL",
        default_value = "z-ai/glm-5.2",
        help_heading = "General"
    )]
    pub model: String,

    /// Maximum number of turns the agent can take before the program exits
    #[arg(
        long,
        env = "CODE_REVIEW_AGENT_MAX_TURNS",
        default_value_t = 20,
        value_parser = parse_u32_or_default::<20>,
        help_heading = "Safety"
    )]
    pub max_turns: u32,

    /// Maximum number of retries for a failed provider request within a single turn.
    /// The original attempt is not counted; `0` disables retries.
    #[arg(
        long,
        env = "CODE_REVIEW_AGENT_MAX_RETRIES",
        default_value_t = 3,
        value_parser = parse_u32_or_default::<3>,
        help_heading = "Safety"
    )]
    pub max_retries: u32,

    /// Optional GitLab access token used to post the review as an MR comment.
    /// When absent, posting is disabled. Also accepts `GITLAB_API_TOKEN` if this
    /// flag / `CODE_REVIEW_AGENT_GITLAB_TOKEN` is unset.
    #[arg(
        long,
        env = "CODE_REVIEW_AGENT_GITLAB_TOKEN",
        hide_env_values = true,
        help_heading = "Publishing"
    )]
    pub gitlab_token: Option<String>,

    /// Optional path to a file containing the system prompt. When absent, the
    /// CODE_REVIEW_AGENT.md file is checked. If neither is present, a default
    /// system prompt is used.
    #[arg(
        long,
        env = "CODE_REVIEW_AGENT_SYSTEM_PROMPT_PATH",
        help_heading = "System prompt"
    )]
    pub system_prompt_path: Option<String>,

    /// Extra arguments passed to the model provider in the request body.
    #[command(flatten)]
    pub model_options: ModelOptions,
}

/// Model-specific options that are merged into the request body sent to the provider.
#[derive(Debug, Clone, Default, clap::Args)]
pub struct ModelOptions {
    /// Reasoning effort for the model, e.g. `low`, `medium`, `high` or other model-specific values
    #[arg(
        long,
        env = "CODE_REVIEW_AGENT_REASONING_EFFORT",
        help_heading = "Model options"
    )]
    pub reasoning_effort: Option<String>,

    /// Maximum number of tokens the model may generate
    #[arg(
        long,
        env = "CODE_REVIEW_AGENT_MAX_COMPLETION_TOKENS",
        help_heading = "Model options"
    )]
    pub max_completion_tokens: Option<u32>,
}

impl ModelOptions {
    pub fn to_request_fields(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut fields = serde_json::Map::new();

        if let Some(effort) = self
            .reasoning_effort
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            fields.insert(
                "reasoning".to_string(),
                serde_json::json!({ "effort": effort }),
            );
        }
        if let Some(max_completion_tokens) = self.max_completion_tokens {
            fields.insert(
                "max_completion_tokens".to_string(),
                serde_json::json!(max_completion_tokens),
            );
        }

        fields
    }
}

/// Parses a numeric value, treating empty or whitespace-only input as `DEFAULT`.
fn parse_u32_or_default<const DEFAULT: u32>(s: &str) -> Result<u32, String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        Ok(DEFAULT)
    } else {
        trimmed
            .parse::<u32>()
            .map_err(|_| format!("invalid numeric value: {trimmed:?}"))
    }
}

impl Configuration {
    pub fn new() -> color_eyre::Result<Self> {
        Self::parse().normalized()
    }

    fn normalized(mut self) -> color_eyre::Result<Self> {
        self.gitlab_token = self
            .gitlab_token
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                std::env::var("GITLAB_API_TOKEN")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
            });
        self.system_prompt_path = self.system_prompt_path.filter(|s| !s.trim().is_empty());
        self.model_options.reasoning_effort = self
            .model_options
            .reasoning_effort
            .take()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        if self.api_key.trim().is_empty() {
            return Err(color_eyre::eyre::eyre!(
                "API key must not be empty. Please provide --api-key or set CODE_REVIEW_AGENT_API_KEY."
            ));
        }

        self.api_endpoint = self.api_endpoint.trim().to_string();
        if self.api_endpoint.is_empty() {
            return Err(color_eyre::eyre::eyre!(
                "API endpoint must not be empty. Please provide --api-endpoint or set CODE_REVIEW_AGENT_API_ENDPOINT."
            ));
        }
        if let Err(e) = Url::parse(&self.api_endpoint) {
            return Err(color_eyre::eyre::eyre!(
                "API endpoint '{}' is not a valid URL: {e}",
                self.api_endpoint
            ));
        }

        self.model = self.model.trim().to_string();
        if self.model.is_empty() {
            return Err(color_eyre::eyre::eyre!(
                "Model must not be empty. Please provide --model or set CODE_REVIEW_AGENT_MODEL."
            ));
        }

        Ok(self)
    }
}

impl std::fmt::Debug for Configuration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Configuration")
            .field("api_key", &"<redacted>")
            .field(
                "gitlab_token",
                &self.gitlab_token.as_ref().map(|_| "<redacted>"),
            )
            // ... non-secret fields
            .finish()
    }
}
