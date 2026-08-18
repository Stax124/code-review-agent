use clap::Parser;

/// Configuration for the code review agent.
///
/// Values can be set via CLI flags or `CODE_REVIEW_AGENT_*` environment variables.
/// CLI flags take precedence over env vars, which take precedence over defaults.
#[derive(Parser, Debug)]
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
        help_heading = "Safety"
    )]
    pub max_turns: u32,

    /// Maximum number of retries for a failed provider request within a single turn.
    /// The original attempt is not counted; `0` disables retries.
    #[arg(
        long,
        env = "CODE_REVIEW_AGENT_MAX_RETRIES",
        default_value_t = 3,
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

        if self.api_key.trim().is_empty() {
            return Err(color_eyre::eyre::eyre!(
                "API key must not be empty. Please provide --api-key or set CODE_REVIEW_AGENT_API_KEY."
            ));
        }

        Ok(self)
    }
}
