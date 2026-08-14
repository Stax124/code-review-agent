use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::io::AsyncBufReadExt;

use crate::tools::AgentTool;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ReadFileToolArgs {
    file: String,
    #[serde(default)]
    start_line: Option<usize>,
    #[serde(default)]
    end_line: Option<usize>,
}

pub struct ReadFileTool {}

impl ReadFileTool {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl AgentTool for ReadFileTool {
    fn name(&self) -> &'static str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read the contents of a specific file. Allows either reading the entire file or a specific range of lines (1-based, inclusive). Each returned line is prefixed with its line number so you can orient yourself."
    }

    fn properties_schema(&self) -> Value {
        json!({
            "file": {
                "type": "string",
                "description": "The file path to read."
            },
            "start_line": {
                "type": "integer",
                "minimum": 1,
                "description": "The starting line number (1-based) to read from. Optional.",
            },
            "end_line": {
                "type": "integer",
                "minimum": 1,
                "description": "The ending line number (1-based) to read to. Optional.",
            }
        })
    }

    fn required_parameters(&self) -> Vec<&'static str> {
        vec!["file"]
    }

    async fn execute(
        &self,
        args: Value,
    ) -> Result<(String, String), Box<dyn std::error::Error + Send + Sync>> {
        let args = serde_json::from_value::<ReadFileToolArgs>(args)?;
        let path = std::path::Path::new(&args.file);

        // Check if the file exists
        if !path.exists() {
            return Err(format!("The file '{}' does not exist.", args.file).into());
        }

        // Check if the file is ignored by any .gitignore file
        if is_ignored_by_gitignore(path, false)? {
            return Err(format!("The file '{}' is ignored by .gitignore.", args.file).into());
        }

        // Check if it is a symbolic link, if yes, follow the link but return error if it is outside of our boundary.
        let canonical_path = std::fs::canonicalize(path)?;
        let current_dir = std::env::current_dir()?;
        if !canonical_path.starts_with(&current_dir) {
            return Err(format!(
                "The file '{}' is outside of the current directory boundary.",
                args.file
            )
            .into());
        }

        // Check that the file is not a directory
        if canonical_path.is_dir() {
            return Err(format!("The path '{}' is a directory, not a file.", args.file).into());
        }

        // Check that the file size is not too large (e.g., larger than 1MB)
        let metadata = std::fs::metadata(&canonical_path)?;
        if metadata.len() > 1_000_000 {
            return Err(format!(
                "The file '{}' is too large ({} bytes). Maximum allowed size is 1MB.",
                args.file,
                metadata.len()
            )
            .into());
        }

        // Validate line parameters up front. Lines are 1-based and inclusive.
        if let Some(0) = args.start_line {
            return Err("start_line must be >= 1.".into());
        }
        if let Some(0) = args.end_line {
            return Err("end_line must be >= 1.".into());
        }
        if let (Some(s), Some(e)) = (args.start_line, args.end_line)
            && e < s
        {
            return Err(format!("end_line ({}) must be >= start_line ({}).", e, s).into());
        }

        // Stream the file line-by-line instead of buffering it all into memory.
        let file = tokio::fs::File::open(&canonical_path).await?;
        let reader = tokio::io::BufReader::new(file);
        let mut lines = reader.lines();

        let start_line = args.start_line.unwrap_or(1);
        let end_line = args.end_line;

        let mut selected: Vec<String> = Vec::new();
        let mut line_no: usize = 0;
        let mut total_lines: usize = 0;
        let mut stopped_early = false;

        while let Some(line) = lines.next_line().await? {
            line_no += 1;
            if let Some(end) = end_line
                && line_no > end
            {
                // We've read past the requested range; no need to keep going.
                stopped_early = true;
                break;
            }
            if line_no >= start_line {
                selected.push(format!("{:>6}: {}", line_no, line));
            }
        }

        // If we didn't stop early we reached EOF, so `line_no` is the true total.
        if !stopped_early {
            total_lines = line_no;
            // `start_line` is out of range if it exceeds the number of lines.
            // For an empty file (0 lines) a default `start_line` of 1 is still
            // valid and yields an empty result, so clamp the floor to 1.
            if start_line > total_lines.max(1) {
                return Err(format!(
                    "The file '{}' has {} line(s). start_line {} is out of range.",
                    args.file, total_lines, start_line
                )
                .into());
            }
        }

        let output = selected.join("\n");

        let summary = if stopped_early {
            // We bailed out before EOF, so the true total line count is unknown.
            format!(
                "read_file: {} (lines {}-{})",
                args.file,
                start_line,
                end_line.unwrap()
            )
        } else if total_lines == 0 {
            format!("read_file: {} (empty file)", args.file)
        } else {
            let effective_end = end_line.unwrap_or(total_lines).min(total_lines);
            if start_line == 1 && end_line.is_none() {
                format!("read_file: {} ({} lines)", args.file, total_lines)
            } else {
                format!(
                    "read_file: {} (lines {}-{} of {})",
                    args.file, start_line, effective_end, total_lines
                )
            }
        };

        Ok((output, summary))
    }
}

fn is_ignored_by_gitignore(
    path: &std::path::Path,
    is_dir: bool,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    use std::path::{Path, PathBuf};

    let current_dir = std::env::current_dir()?;
    let canonical = std::fs::canonicalize(path)?;

    // Collect every ancestor directory of the file within current directory
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut current = if is_dir {
        canonical.clone()
    } else {
        canonical
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| canonical.clone())
    };
    while current.starts_with(&current_dir) {
        dirs.push(current.clone());
        if current == current_dir {
            break;
        }
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => break,
        }
    }

    // Innermost match wins: iterate from the directory closest to the file
    // outward to the workspace root.
    for dir in dirs {
        let gitignore_path = dir.join(".gitignore");
        if !gitignore_path.is_file() {
            continue;
        }
        // The matcher is rooted at the directory containing the .gitignore so
        // that patterns are interpreted relative to that directory, matching
        // git's semantics.
        let mut builder = ignore::gitignore::GitignoreBuilder::new(&dir);
        builder.add(&gitignore_path);
        let matcher = builder.build()?;
        let m = matcher.matched(&canonical, is_dir);
        match m {
            ignore::Match::Ignore(_) => return Ok(true),
            ignore::Match::Whitelist(_) => return Ok(false),
            ignore::Match::None => {}
        }
    }

    Ok(false)
}
