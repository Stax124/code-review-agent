use std::{collections::HashMap, io, path::Path};

use async_trait::async_trait;
use grep::searcher::{Searcher, SearcherBuilder, Sink, SinkContext, SinkMatch};
use ignore::Walk;
use serde_json::{Value, json};

use crate::tools::AgentTool;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SearchInDirectoryToolArgs {
    subdirectory: Option<String>,
    search_pattern: String,
    context_lines: Option<usize>,
}

/// A single line reported by the searcher, either a match or a context line.
#[derive(Debug, Clone)]
pub struct SearchLine {
    pub line_number: u64,
    pub is_match: bool,
    pub line: String,
}

/// A custom [`Sink`] that collects both matching lines and surrounding context lines
struct ContextCollectingSink {
    lines: Vec<SearchLine>,
}

impl ContextCollectingSink {
    fn new() -> Self {
        Self { lines: Vec::new() }
    }
}

impl Sink for ContextCollectingSink {
    type Error = io::Error;

    fn matched(&mut self, _searcher: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, io::Error> {
        let line_number = mat
            .line_number()
            .ok_or_else(|| io::Error::other("line numbers not enabled"))?;
        let line = String::from_utf8_lossy(mat.bytes()).into_owned();
        self.lines.push(SearchLine {
            line_number,
            is_match: true,
            line,
        });
        Ok(true)
    }

    fn context(&mut self, _searcher: &Searcher, ctx: &SinkContext<'_>) -> Result<bool, io::Error> {
        let line_number = ctx
            .line_number()
            .ok_or_else(|| io::Error::other("line numbers not enabled"))?;
        let line = String::from_utf8_lossy(ctx.bytes()).into_owned();
        self.lines.push(SearchLine {
            line_number,
            is_match: false,
            line,
        });
        Ok(true)
    }
}

pub struct SearchInDirectoryTool {}

impl SearchInDirectoryTool {
    pub fn new() -> Self {
        Self {}
    }

    pub fn search_in_directory(
        base_path: &Path,
        pattern: &str,
        context_lines: usize,
    ) -> color_eyre::Result<HashMap<String, Vec<SearchLine>>> {
        let mut matches: HashMap<String, Vec<SearchLine>> = HashMap::new();

        for entry in Walk::new(base_path) {
            let entry = entry?;
            let file_path = entry.path();
            if file_path.is_file() {
                let file_matches = Self::search_in_file(file_path, pattern, context_lines)?;
                if !file_matches.is_empty() {
                    matches
                        .entry(file_path.to_string_lossy().to_string())
                        .or_default()
                        .extend(file_matches);
                }
            }
        }

        Ok(matches)
    }

    pub fn search_in_file(
        file_path: &Path,
        pattern: &str,
        context_lines: usize,
    ) -> color_eyre::Result<Vec<SearchLine>> {
        let matcher = grep::regex::RegexMatcher::new(pattern)?;
        let mut sink = ContextCollectingSink::new();
        let mut searcher = SearcherBuilder::new()
            .before_context(context_lines)
            .after_context(context_lines)
            .build();

        searcher.search_path(&matcher, file_path, &mut sink)?;

        // Sort by line number and deduplicate
        let mut lines = sink.lines;
        lines.sort_by_key(|line| line.line_number);
        lines.dedup_by(|a, b| {
            if a.line_number == b.line_number {
                // Prefer the match entry over the context entry.
                if b.is_match {
                    a.is_match = true;
                }
                true
            } else {
                false
            }
        });

        Ok(lines)
    }
}

#[async_trait]
impl AgentTool for SearchInDirectoryTool {
    fn name(&self) -> &'static str {
        "search_in_directory"
    }

    fn description(&self) -> &str {
        "Search for a pattern in the current directory."
    }

    fn properties_schema(&self) -> Value {
        json!({
            "subdirectory": {
                "type": "string",
                "description": "Optional subdirectory to search in. If not provided, the current directory will be searched."
            },
            "search_pattern": {
                "type": "string",
                "description": "The pattern to search for."
            },
            "context_lines": {
                "type": "integer",
                "description": "Optional number of context lines to include before and after each match. Default is 0.",
            }
        })
    }

    fn required_parameters(&self) -> Vec<&'static str> {
        vec!["search_pattern"]
    }

    async fn execute(
        &self,
        args: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let args = serde_json::from_value::<SearchInDirectoryToolArgs>(args)?;
        let path = &args.subdirectory.unwrap_or_else(|| ".".into());

        // Verify that the subdirectory is at or below the current directory to prevent directory traversal attacks
        let current_dir = std::env::current_dir()?.canonicalize()?;
        let full_path = current_dir.join(path).canonicalize()?;
        if !full_path.starts_with(&current_dir) {
            return Err(format!(
                "The subdirectory '{}' is outside of the current directory boundary.",
                path
            )
            .into());
        }

        let matches = Self::search_in_directory(
            &full_path,
            &args.search_pattern,
            args.context_lines.unwrap_or_default(),
        )?;

        // Ripgrep like formatting
        //
        // src/file.rs
        // 17-    let before = "context line";
        // 18:    let test = "Some matching line";
        // 19-    let after = "context line";
        let mut output = String::new();
        for (file, lines) in matches {
            output.push_str(&format!("{}:\n", file));
            for line in lines {
                let separator = if line.is_match { ':' } else { '-' };
                output.push_str(&format!("{}{} {}", line.line_number, separator, line.line));
            }
            output.push('\n');
        }

        Ok(output)
    }
}
