use colored::Colorize;
use std::io::Write;

use crate::agent::response::StreamEvent;

/// Represents the current section of the output being printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamSection {
    /// Nothing has been printed yet this turn.
    None,
    /// Currently printing reasoning (chain-of-thought) tokens.
    Reasoning,
    /// Currently printing visible response content tokens.
    Content,
}

/// Stateful printer for live streaming output.
pub struct StreamPrinter {
    section: StreamSection,
    printed_any: bool,
}

impl StreamPrinter {
    pub fn new() -> Self {
        Self {
            section: StreamSection::None,
            printed_any: false,
        }
    }

    pub fn on_event(&mut self, event: StreamEvent) {
        match event {
            StreamEvent::ReasoningDelta(text) => {
                if text.is_empty() {
                    return;
                }
                self.begin_section(StreamSection::Reasoning);
                print!("{}", text.dimmed());
                let _ = std::io::stdout().flush();
            }
            StreamEvent::ContentDelta(text) => {
                if text.is_empty() {
                    return;
                }
                self.begin_section(StreamSection::Content);
                print!("{}", text.green());
                let _ = std::io::stdout().flush();
            }
            StreamEvent::Done => {
                if self.printed_any {
                    println!();
                    let _ = std::io::stdout().flush();
                }
            }
        }
    }

    /// Transition to `target`, printing a header (and a separating newline if
    /// switching from a different section) the first time `target` is entered.
    pub fn begin_section(&mut self, target: StreamSection) {
        if self.section == target {
            return;
        }
        // Close the previous section's line before starting a new header.
        if self.printed_any {
            println!();
        }
        match target {
            StreamSection::Reasoning => {
                print!("{}: ", "Reasoning".blue().bold());
            }
            StreamSection::Content => {
                print!("{}: ", "Response".green().bold());
            }
            StreamSection::None => {}
        }
        self.section = target;
        self.printed_any = true;
        let _ = std::io::stdout().flush();
    }
}
