use super::ReviewPublisher;
use async_trait::async_trait;
use colored::Colorize;

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
        let mr_iid = std::env::var("CI_MERGE_REQUEST_IID").ok();

        let (Some(project_id), Some(mr_iid)) = (project_id, mr_iid) else {
            tracing::warn!(
                "CI_PROJECT_ID / CI_MERGE_REQUEST_IID not set; not running in a GitLab MR pipeline, review posting is disabled."
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
}

#[async_trait]
impl ReviewPublisher for GitlabPublisher {
    async fn publish(&self, review: &str) -> color_eyre::Result<()> {
        let url = format!(
            "{}/projects/{}/merge_requests/{}/notes",
            self.api_base, self.project_id, self.mr_iid
        );

        let response = self
            .http_client
            .post(&url)
            .json(&serde_json::json!({ "body": review }))
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
        println!(
            "{}: Review posted to merge request !{}",
            "GitLab".green().bold(),
            self.mr_iid
        );
        Ok(())
    }
}
