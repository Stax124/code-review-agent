use std::path::Path;

use crate::{
    constants::DEFAULT_PROMPT,
    utils::{
        git::list_commits_on_this_branch,
        list_directory::{list_directory, prettify_directory_listing},
    },
};

pub fn load_system_prompt_from_file(path: Option<&Path>) -> color_eyre::Result<Option<String>> {
    let path = match path {
        Some(p) => p,
        None => Path::new("CODE_REVIEW_AGENT.md"),
    };

    let canonical_path = match path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            tracing::info!(
                "System prompt file at {} could not be canonicalized: {}. Using default prompt.",
                path.display(),
                e
            );
            return Ok(None);
        }
    };

    // Make sure that the file exists
    if !canonical_path.exists() {
        tracing::info!(
            "System prompt file not found at {}. Using default prompt.",
            path.display()
        );
        return Ok(None);
    }

    // Make sure that the path is a file
    let metadata = std::fs::metadata(&canonical_path)?;
    if !metadata.is_file() {
        tracing::info!(
            "System prompt path at {} is not a file. Using default prompt.",
            canonical_path.display()
        );
        return Ok(None);
    }

    // Make sure that the file is not empty
    if metadata.len() == 0 {
        tracing::info!(
            "System prompt file at {} is empty. Using default prompt.",
            canonical_path.display()
        );
        return Ok(None);
    }

    // Make sure that the file is within this path and not a symlink to somewhere else outside this scope

    let current_dir = match std::env::current_dir()?.canonicalize() {
        Ok(dir) => dir,
        Err(e) => {
            tracing::info!(
                "Current directory could not be canonicalized: {}. Using default prompt.",
                e
            );
            return Ok(None);
        }
    };
    if !canonical_path.starts_with(&current_dir) {
        tracing::info!(
            "System prompt file at {} is outside the current directory. Using default prompt.",
            canonical_path.display()
        );
        return Ok(None);
    }

    let prompt = match std::fs::read_to_string(&canonical_path) {
        Ok(s) => s,
        Err(e) => {
            tracing::info!(
                "System prompt file at {} could not be read: {}. Using default prompt.",
                canonical_path.display(),
                e
            );
            return Ok(None);
        }
    };

    tracing::info!(
        "Successfully loaded system prompt from {}",
        canonical_path.display()
    );
    Ok(Some(prompt))
}

pub fn generate_system_prompt(
    full_diff: &str,
    base_branch: &str,
    system_prompt_path: Option<&Path>,
) -> color_eyre::Result<String> {
    let project_structure = prettify_directory_listing(&list_directory(Path::new("."), None)?);
    let commits_on_this_branch = list_commits_on_this_branch(base_branch)
        .unwrap_or_default()
        .iter()
        .map(|(id, message)| format!("{} {}", id, message))
        .collect::<Vec<String>>()
        .join("\n");

    let prompt = load_system_prompt_from_file(system_prompt_path)?
        .unwrap_or_else(|| DEFAULT_PROMPT.to_string())
        .replace("{project_structure}", &project_structure)
        .replace("{commits_on_this_branch}", &commits_on_this_branch)
        .replace("{full_diff}", full_diff);

    Ok(prompt)
}
