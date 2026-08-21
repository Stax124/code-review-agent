use crate::{
    config::Configuration,
    tools::{
        file_tree::FileTreeTool, get_commit_diff::GetCommitDiffTool,
        list_directory::ListDirectoryTool, read_file::ReadFileTool,
        search_in_directory::SearchInDirectoryTool,
    },
};

pub mod client;
pub mod response;
pub mod telemetry;

pub fn build_agent(
    config: &Configuration,
    system_prompt: &str,
) -> color_eyre::Result<client::AgentClient> {
    let mut agent_client = client::AgentClient::builder()
        .with_model(&config.model)
        .with_endpoint(&config.api_endpoint)
        .with_api_key(&config.api_key)
        .with_system_message(system_prompt)
        .with_temperature(0.0)
        .with_extra_request_fields(config.model_options.to_request_fields())
        .with_tool(Box::new(GetCommitDiffTool::new()))
        .with_tool(Box::new(ListDirectoryTool::new()))
        .with_tool(Box::new(FileTreeTool::new()))
        .with_tool(Box::new(SearchInDirectoryTool::new()))
        .with_tool(Box::new(ReadFileTool::new()))
        .build()?;

    agent_client.add_message(client::AgentClientMessage::User(
        client::AgentClientMessageUser {
            role: "user".to_string(),
            content: "Please provide a code review for the changes listed in the diff".to_string(),
        },
    ));

    Ok(agent_client)
}
