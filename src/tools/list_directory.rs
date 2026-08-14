use crate::utils::list_directory::prettify_directory_listing;
use crate::{tools::AgentTool, utils::list_directory::list_directory};

use async_trait::async_trait;
use serde_json::{Value, json};

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ListDirectoryToolArgs {
    subdirectory: Option<String>,
}

pub struct ListDirectoryTool {}

impl ListDirectoryTool {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl AgentTool for ListDirectoryTool {
    fn name(&self) -> &'static str {
        "list_directory"
    }

    fn description(&self) -> &str {
        "List the contents of a directory. Files will have their sizes displayed in [{filesize}] format, directories will have a trailing slash. Does not recurse into subdirectories."
    }

    fn properties_schema(&self) -> Value {
        json!({
            "subdirectory": {
                "type": "string",
                "description": "Optional subdirectory to list. If not provided, the current directory will be listed."
            }
        })
    }

    fn required_parameters(&self) -> Vec<&'static str> {
        vec![]
    }

    async fn execute(
        &self,
        args: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let args = serde_json::from_value::<ListDirectoryToolArgs>(args)?;
        let base_path = std::env::current_dir()?;
        let entries = list_directory(&base_path, args.subdirectory.as_deref())?;

        let output = prettify_directory_listing(&entries);
        Ok(output)
    }
}
