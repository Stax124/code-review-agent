pub mod gitlab;

use async_trait::async_trait;

/// Publishes a completed code review.
#[async_trait]
pub trait ReviewPublisher: Send + Sync {
    async fn publish(&self, review: &str) -> color_eyre::Result<()>;
}

/// Detect the hosting platform from the CI environment and build the matching publisher
pub fn detect_and_build(
    token: Option<&str>,
) -> color_eyre::Result<Option<Box<dyn ReviewPublisher>>> {
    if token.is_none() {
        tracing::debug!("No token provided; review posting is disabled.");
        return Ok(None);
    }

    let gitlab_publisher = gitlab::GitlabPublisher::from_env(token.unwrap_or_default())?;
    if let Some(publisher) = gitlab_publisher {
        return Ok(Some(Box::new(publisher)));
    }

    tracing::debug!(
        "A token was provided but no supported CI platform was detected; review posting is disabled."
    );
    Ok(None)
}
