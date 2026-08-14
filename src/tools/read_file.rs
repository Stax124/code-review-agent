use async_trait::async_trait;
use serde_json::{Value, json};

use crate::tools::AgentTool;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ReadFileToolArgs {
    file: String,
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
        "Read the contents of a specific file."
    }

    fn properties_schema(&self) -> Value {
        json!({
            "file": {
                "type": "string",
                "description": "The file path to read."
            }
        })
    }

    fn required_parameters(&self) -> Vec<&'static str> {
        vec!["file"]
    }

    async fn execute(
        &self,
        args: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
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

        let content = tokio::fs::read_to_string(canonical_path).await?;
        Ok(content)
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
