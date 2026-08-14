use colored::Colorize;

use crate::{
    agent::client::{AgentClientMessage, AgentClientMessageUser},
    stream::StreamPrinter,
    utils::{
        git::{determine_base_branch, get_branch_diff_against_base},
        prompt::generate_system_prompt,
    },
};

mod agent;
mod constants;
mod stream;
mod tools;
mod utils;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    dotenvy::dotenv().ok();
    utils::logging::init_logger();

    let base_branch = determine_base_branch()?;
    let api_key = std::env::var("OPENROUTER_API_KEY").map_err(|_| {
        color_eyre::eyre::eyre!("OPENROUTER_API_KEY environment variable is not set.")
    })?;
    let full_diff = get_branch_diff_against_base(&base_branch)?;
    let system_prompt = generate_system_prompt(&full_diff, &base_branch)?;
    let model = std::env::var("OPENROUTER_MODEL")
        .unwrap_or_else(|_| "deepseek/deepseek-v4-flash-0731".to_string());

    let mut agent_client = agent::client::AgentClient::builder()
        .with_model(&model)
        .with_endpoint("https://openrouter.ai/api/v1/chat/completions")
        .with_api_key(&api_key)
        .with_system_message(&system_prompt)
        .with_temperature(0.0)
        .with_tool(Box::new(tools::get_commit_diff::GetCommitDiffTool::new()))
        .with_tool(Box::new(tools::list_directory::ListDirectoryTool::new()))
        .with_tool(Box::new(
            tools::search_in_directory::SearchInDirectoryTool::new(),
        ))
        .with_tool(Box::new(tools::read_file::ReadFileTool::new()))
        .build()?;

    agent_client.add_message(AgentClientMessage::User(AgentClientMessageUser {
        role: "user".to_string(),
        content: "Please provide a code review for the changes listed in the diff".to_string(),
    }));

    let max_turns = std::env::var("MAX_AGENT_TURNS")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(20);
    let turn_reminders = [5, 10, 15];
    let mut turn = 1;
    let mut ran_out_of_turns = false;
    while turn <= max_turns {
        if turn_reminders.contains(&(max_turns - turn)) {
            // Insert a reminder message to the agent after every 5 turns to encourage it to complete the task.
            agent_client.add_message(AgentClientMessage::User(AgentClientMessageUser {
                role: "user".to_string(),
                content: format!(
                    "[SYSTEM NOTE]: You have {} turns left to complete the task",
                    max_turns - turn
                ),
            }));
        }

        // If this is the last turn add a final reminder to the agent to complete the task.
        if turn == max_turns {
            agent_client.add_message(AgentClientMessage::User(AgentClientMessageUser {
                role: "user".to_string(),
                content: "[SYSTEM NOTE]: This is your last turn to complete the task. Please provide a final response.".to_string(),
            }));
        }

        tracing::info!("{}", format!("── Turn {turn}/{max_turns} ──").cyan().bold());
        let mut printer = StreamPrinter::new();

        let (response, should_continue) =
            agent_client.send(|event| printer.on_event(event)).await?;
        tracing::debug!("Response: {}", serde_json::to_string(&response)?);

        for choice in response.choices.iter() {
            println!(
                "{}: {} | {}: {}",
                "Tool calls".yellow().bold(),
                choice.message.tool_calls.clone().unwrap_or_default().len(),
                "Finish reason".red().bold(),
                choice.finish_reason
            );
        }

        if !should_continue {
            break;
        }
        turn += 1;
        if turn > max_turns {
            ran_out_of_turns = true;
        }
    }

    if ran_out_of_turns {
        tracing::warn!(
            "Agent interaction reached the maximum number of turns ({}) without completing the task.",
            max_turns
        );
    } else {
        tracing::info!("Agent interaction completed. Total turns: {}", turn);
    }

    Ok(())
}
