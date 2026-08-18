use color_eyre::eyre::eyre;

pub struct Configuration {
    /// API key sent in the `Authorization` header
    pub api_key: String,
    /// OpenAI API compatible endpoint, e.g. `https://openrouter.ai/api/v1/chat/completions`
    pub api_endpoint: String,
    /// Model to use for code review, e.g. `z-ai/glm-5.2`
    pub model: String,

    /// Maximum number of turns the agent can take before the program exits. This is a safety measure to prevent infinite loops.
    pub max_turns: u32,

    /// Maximum number of retries for a failed provider request within a single turn.
    /// The original attempt is not counted; `0` disables retries.
    pub max_retries: u32,

    /// Optional GitLab access token used to post the review as an MR comment. When absent, posting is disabled.
    pub gitlab_token: Option<String>,

    /// Optional path to a file containing the system prompt. When absent, the CODE_REVIEW_AGENT.md
    /// file is checked for a system prompt. If neither is present, a default system prompt is used.
    pub system_prompt_path: Option<String>,
}

impl Configuration {
    pub fn new() -> color_eyre::Result<Self> {
        // General
        let api_key = std::env::var("CODE_REVIEW_AGENT_API_KEY")
            .map_err(|_| eyre!("CODE_REVIEW_AGENT_API_KEY environment variable not set"))?;
        let api_endpoint = std::env::var("CODE_REVIEW_AGENT_API_ENDPOINT")
            .unwrap_or_else(|_| "https://openrouter.ai/api/v1/chat/completions".to_string());
        let model =
            std::env::var("CODE_REVIEW_AGENT_MODEL").unwrap_or_else(|_| "z-ai/glm-5.2".to_string());

        // Safety
        let max_turns = std::env::var("CODE_REVIEW_AGENT_MAX_TURNS")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(20);
        let max_retries = std::env::var("CODE_REVIEW_AGENT_MAX_RETRIES")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(3);

        // Publishing
        let gitlab_token = std::env::var("CODE_REVIEW_AGENT_GITLAB_TOKEN")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                std::env::var("GITLAB_API_TOKEN")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
            });

        // System prompt
        let system_prompt_path = std::env::var("CODE_REVIEW_AGENT_SYSTEM_PROMPT_PATH")
            .ok()
            .filter(|s| !s.trim().is_empty());

        Ok(Configuration {
            api_key,
            api_endpoint,
            model,
            max_turns,
            max_retries,
            gitlab_token,
            system_prompt_path,
        })
    }
}
