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

        Ok(Configuration {
            api_key,
            api_endpoint,
            model,
            max_turns,
        })
    }
}
