use super::ReviewPublisher;
use crate::agent::telemetry::Telemetry;
use async_trait::async_trait;

const DEFAULT_API_BASE: &str = "https://gitlab.com/api/v4";

/// Posts the review as a note on a GitLab merge request.
pub struct GitlabPublisher {
    http_client: reqwest::Client,
    api_base: String,
    project_id: String,
    mr_iid: String,
}

impl GitlabPublisher {
    /// Build a publisher from GitLab CI environment variables
    pub fn from_env(token: &str) -> color_eyre::Result<Option<Self>> {
        let project_id = std::env::var("CI_PROJECT_ID").ok();

        // We may not be running in a merge request pipeline, but we can still try to get the merge request IID from the CI_OPEN_MERGE_REQUESTS variable if it's set.
        let mr_iid = std::env::var("CI_MERGE_REQUEST_IID")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                std::env::var("CI_OPEN_MERGE_REQUESTS")
                    .ok()
                    .and_then(|mr_list| {
                        mr_list
                            .split(',')
                            .next()?
                            .split('!')
                            .nth(1)
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
                    })
            });

        let (Some(project_id), Some(mr_iid)) = (project_id, mr_iid) else {
            tracing::warn!(
                "CI_PROJECT_ID / (CI_MERGE_REQUEST_IID, CI_OPEN_MERGE_REQUESTS) not set; review posting is disabled."
            );
            return Ok(None);
        };

        let api_base = std::env::var("CI_API_V4_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_API_BASE.to_string())
            .trim_end_matches('/')
            .to_string();

        let publisher = Self::new(token, api_base, project_id, mr_iid)?;
        Ok(Some(publisher))
    }

    pub fn new(
        token: &str,
        api_base: String,
        project_id: String,
        mr_iid: String,
    ) -> color_eyre::Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "PRIVATE-TOKEN",
            token.parse().map_err(|_| {
                color_eyre::eyre::eyre!("Failed to parse GitLab token into header value")
            })?,
        );

        let http_client = reqwest::Client::builder()
            .default_headers(headers)
            .connect_timeout(std::time::Duration::from_secs(5))
            .read_timeout(std::time::Duration::from_secs(20))
            .build()
            .map_err(|e| color_eyre::eyre::eyre!("Failed to build HTTP client: {}", e))?;

        Ok(Self {
            http_client,
            api_base,
            project_id,
            mr_iid,
        })
    }

    /// Generates a GitLab comment footer (utilizing markdown) that summarizes the telemetry data, including token usage and cost.
    pub fn generate_summary_from_telemetry(&self, telemetry: &Telemetry) -> String {
        let mut summary = String::new();
        summary.push_str("\n\n---\n\n");
        summary.push_str("### Telemetry\n\n");

        let mut stats_entries: Vec<_> = telemetry.provider_stats.iter().collect();
        stats_entries.sort_by_key(|(k, _)| *k);
        for (provider, stats) in stats_entries {
            summary.push_str(&format!(
                "- **{}**: Prompt tokens: {}, Completion tokens: {}, Total tokens: {}, Total cost: ${:.6}\n",
                provider,
                crate::utils::conversion::tokens_to_human_readable(stats.total_prompt_tokens),
                crate::utils::conversion::tokens_to_human_readable(stats.total_completion_tokens),
                crate::utils::conversion::tokens_to_human_readable(
                    stats.total_prompt_tokens + stats.total_completion_tokens
                ),
                stats.total_cost
            ));
        }

        summary
    }
}

#[async_trait]
impl ReviewPublisher for GitlabPublisher {
    async fn publish(&self, review: &str, telemetry: &Telemetry) -> color_eyre::Result<()> {
        let url = format!(
            "{}/projects/{}/merge_requests/{}/notes",
            self.api_base, self.project_id, self.mr_iid
        );

        let review_with_summary = format!(
            "{}\n{}",
            review,
            self.generate_summary_from_telemetry(telemetry)
        );

        let response = self
            .http_client
            .post(&url)
            .json(&serde_json::json!({ "body": review_with_summary }))
            .send()
            .await?;

        if !response.status().is_success() {
            color_eyre::eyre::bail!(
                "GitLab API request failed with status: {}, Error: {}",
                response.status(),
                response.text().await?
            );
        }

        tracing::info!("Review posted to merge request !{}.", self.mr_iid);
        Ok(())
    }
}
