pub mod get_commit_diff;
pub mod list_directory;
pub mod read_file;
pub mod search_in_directory;

use async_trait::async_trait;
use serde_json::{Value, json};
use std::error::Error;

#[async_trait]
pub trait AgentTool: Send + Sync {
    /// Unique name the model will call.
    fn name(&self) -> &'static str;

    /// Description of what the tool does, for the model to understand.
    fn description(&self) -> &str;

    /// Build the parameters object for the OpenAI / Anthropic / etc. format.
    /// Using schemars keeps it in sync with the Args struct.
    fn properties_schema(&self) -> Value;

    /// Returns a list of required parameter names for the tool. Return empty if there are no required parameters.
    fn required_parameters(&self) -> Vec<&'static str>;

    /// This function will be called by the model when a tool is invoked.
    /// It should return a tuple of (result, display_string), where result is the raw result and display_string is a human-readable string to show to the user.
    async fn execute(&self, args: Value) -> Result<(String, String), Box<dyn Error + Send + Sync>>;

    /// Convenience helper that produces the exact shape you showed.
    fn to_tool_schema(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": self.name(),
                "description": self.description(),
                "parameters": {
                    "type": "object",
                    "properties": self.properties_schema(),
                    "required": self.required_parameters(),
                }
            }
        })
    }
}
