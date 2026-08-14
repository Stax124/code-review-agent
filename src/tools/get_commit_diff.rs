use async_trait::async_trait;
use serde_json::{Value, json};

use crate::{tools::AgentTool, utils::git::get_commit_diff};

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GetCommitDiffToolArgs {
    commit_id: String,
}

pub struct GetCommitDiffTool {}

impl GetCommitDiffTool {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl AgentTool for GetCommitDiffTool {
    fn name(&self) -> &'static str {
        "get_commit_diff"
    }

    fn description(&self) -> &str {
        "Get the patch diff of a specific commit in the current git repository."
    }

    fn properties_schema(&self) -> Value {
        json!({
            "commit_id": {
                "type": "string",
                "description": "The commit ID for which to retrieve the diff."
            }
        })
    }

    fn required_parameters(&self) -> Vec<&'static str> {
        vec!["commit_id"]
    }

    async fn execute(
        &self,
        args: Value,
    ) -> Result<(String, String), Box<dyn std::error::Error + Send + Sync>> {
        let args = serde_json::from_value::<GetCommitDiffToolArgs>(args)?;
        let diff = get_commit_diff(&args.commit_id)?;
        Ok((diff, format!("get_commit_diff: {}", args.commit_id)))
    }
}
