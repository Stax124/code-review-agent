use colored::Colorize;

use crate::utils::conversion::tokens_to_human_readable;

// Statistics for a single provider
pub struct ProviderStats {
    pub total_prompt_tokens: u32,
    pub total_completion_tokens: u32,
    pub total_cost: f64,
}

// Tracks cost and token usage per provider and provides a summary at the end of the run.
pub struct Telemetry {
    pub provider_stats: std::collections::HashMap<String, ProviderStats>,
}

impl Telemetry {
    pub fn new() -> Self {
        Telemetry {
            provider_stats: std::collections::HashMap::new(),
        }
    }

    pub fn update(
        &mut self,
        provider: &str,
        prompt_tokens: u32,
        completion_tokens: u32,
        cost: f64,
    ) {
        let stats = self
            .provider_stats
            .entry(provider.to_string())
            .or_insert(ProviderStats {
                total_prompt_tokens: 0,
                total_completion_tokens: 0,
                total_cost: 0.0,
            });

        stats.total_prompt_tokens += prompt_tokens;
        stats.total_completion_tokens += completion_tokens;
        stats.total_cost += cost;
    }

    pub fn display_summary(&self) {
        println!("{}:", "Telemetry Summary".green().bold());

        let mut stats_entries: Vec<_> = self.provider_stats.iter().collect();
        stats_entries.sort_by_key(|(k, _)| *k);
        for (provider, stats) in stats_entries {
            println!(
                "- {}: Prompt tokens: {}, Completion tokens: {}, Total tokens: {}, Total cost: ${:.6}",
                provider.yellow().bold(),
                tokens_to_human_readable(stats.total_prompt_tokens),
                tokens_to_human_readable(stats.total_completion_tokens),
                tokens_to_human_readable(stats.total_prompt_tokens + stats.total_completion_tokens),
                stats.total_cost
            );
        }
    }
}
