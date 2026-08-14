use std::path::Path;

use crate::{
    constants::DEFAULT_PROMPT,
    utils::{
        git::list_commits_on_this_branch,
        list_directory::{list_directory, prettify_directory_listing},
    },
};

pub fn generate_system_prompt(full_diff: &str, base_branch: &str) -> color_eyre::Result<String> {
    let project_structure = prettify_directory_listing(&list_directory(Path::new("."), None)?);
    let commits_on_this_branch = list_commits_on_this_branch(base_branch)
        .unwrap_or_default()
        .iter()
        .map(|(id, message)| format!("{} {}", id, message))
        .collect::<Vec<String>>()
        .join("\n");

    let prompt = DEFAULT_PROMPT
        .replace("{project_structure}", &project_structure)
        .replace("{commits_on_this_branch}", &commits_on_this_branch)
        .replace("{full_diff}", full_diff);

    Ok(prompt)
}
