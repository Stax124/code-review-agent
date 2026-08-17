use colored::Colorize;

use crate::{
    agent::client::{AgentClientMessage, AgentClientMessageUser},
    stream::StreamPrinter,
    utils::{
        conversion::tokens_to_human_readable,
        git::{determine_base_branch, get_branch_diff_against_base},
        prompt::generate_system_prompt,
    },
};

mod agent;
mod config;
mod constants;
mod publisher;
mod stream;
mod tools;
mod utils;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    dotenvy::dotenv().ok();
    utils::logging::init_logger();

    let base_branch = determine_base_branch()?;
    let config = config::Configuration::new()?;

    let full_diff = get_branch_diff_against_base(&base_branch)?;
    let system_prompt = generate_system_prompt(
        &full_diff,
        &base_branch,
        config
            .system_prompt_path
            .as_deref()
            .map(std::path::Path::new),
    )?;
    let mut agent_client = agent::build_agent(&config, &system_prompt)?;
    let mut telemetry = agent::telemetry::Telemetry::new();

    let turn_reminders = [5, 10, 15];
    let mut turn = 1;
    let mut ran_out_of_turns = false;
    let mut final_review: Option<String> = None;
    while turn <= config.max_turns {
        if turn_reminders.contains(&(config.max_turns - turn)) {
            // Insert a reminder message to the agent after every 5 turns to encourage it to complete the task.
            agent_client.add_message(AgentClientMessage::User(AgentClientMessageUser {
                role: "user".to_string(),
                content: format!(
                    "[SYSTEM NOTE]: You have {} turns left to complete the task",
                    config.max_turns - turn
                ),
            }));
        }

        // If this is the last turn add a final reminder to the agent to complete the task.
        if turn == config.max_turns {
            agent_client.add_message(AgentClientMessage::User(AgentClientMessageUser {
                role: "user".to_string(),
                content: "[SYSTEM NOTE]: This is your last turn to complete the task. Please provide a final response.".to_string(),
            }));
        }

        println!(
            "{}",
            format!("── Turn {}/{} ──", turn, config.max_turns)
                .cyan()
                .bold()
        );
        let mut printer = StreamPrinter::new();

        let (response, should_continue, tool_calls_to_display) =
            agent_client.send(|event| printer.on_event(event)).await?;
        tracing::debug!("Response: {}", serde_json::to_string(&response)?);

        // Tool calls
        println!(
            "{} ({}):",
            "Tool calls".yellow().bold(),
            tool_calls_to_display.len()
        );
        for display in tool_calls_to_display {
            println!("- {}", display);
        }

        // Cost
        println!(
            "{}: Prompt tokens: {}, Completion tokens: {}, Total tokens: {}, Total cost: ${:.6}",
            "Cost".yellow().bold(),
            tokens_to_human_readable(response.usage.prompt_tokens),
            tokens_to_human_readable(response.usage.completion_tokens),
            tokens_to_human_readable(response.usage.total_tokens),
            response.usage.cost
        );
        telemetry.update(
            &response.provider,
            response.usage.prompt_tokens,
            response.usage.completion_tokens,
            response.usage.cost,
        );

        // Provider
        println!("{}: {}", "Routed to".yellow().bold(), response.provider);

        if !should_continue {
            final_review = response
                .choices
                .into_iter()
                .next()
                .and_then(|choice| choice.message.content)
                .filter(|content| !content.trim().is_empty());
            break;
        }
        turn += 1;
        if turn > config.max_turns {
            ran_out_of_turns = true;
        }
    }

    if ran_out_of_turns {
        tracing::warn!(
            "Agent interaction reached the maximum number of turns ({}) without completing the task.",
            config.max_turns
        );
    } else {
        tracing::info!("Agent interaction completed. Total turns: {}", turn);
    }

    telemetry.display_summary();

    // Additive last step: publish the review as an MR comment when configured.
    match (
        final_review,
        publisher::detect_and_build(config.gitlab_token.as_deref())?,
    ) {
        (Some(review), Some(publisher)) => {
            if let Err(e) = publisher.publish(&review, &telemetry).await {
                tracing::error!("Failed to publish review: {}", e);
            }
        }
        (None, _) => {
            tracing::warn!("No final review was produced; skipping review publishing.");
        }
        (Some(_), None) => {
            tracing::info!("No review publisher configured; skipping review publishing.");
        }
    }

    Ok(())
}
