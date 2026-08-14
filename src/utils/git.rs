use std::process::Command;

/// Determine the base branch to diff the current branch against.
///
/// Resolution order:
/// 1. `CI_MERGE_REQUEST_TARGET_BRANCH_NAME` env var (GitLab CI).
/// 2. The remote's default branch, via `git symbolic-ref refs/remotes/origin/HEAD`
///    (returns e.g. `origin/main`). This lets the bot work even when it is run
///    directly on the default branch: local commits ahead of the remote plus
///    uncommitted changes are then reviewed.
/// 3. A local `main` or `master` branch, probed via `git rev-parse`.
pub fn determine_base_branch() -> color_eyre::Result<String> {
    // Try to parse it from the Gitlab CI environment variable
    if let Ok(base_branch) = std::env::var("CI_MERGE_REQUEST_TARGET_BRANCH_NAME")
        && !base_branch.is_empty()
    {
        return Ok(base_branch);
    }

    // Ask git for the remote's default branch. `symbolic-ref --short` on
    // `refs/remotes/origin/HEAD` yields e.g. `origin/main`.
    if let Ok(output) = Command::new("git")
        .args(["symbolic-ref", "--short", "refs/remotes/origin/HEAD"])
        .output()
    {
        let base = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if output.status.success() && !base.is_empty() {
            return Ok(base);
        }
    }

    // If not found, check whether the primary branch is "main" or "master" and
    // return that as the base branch.
    if Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "main"])
        .output()?
        .status
        .success()
    {
        return Ok("main".to_string());
    }

    if Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "master"])
        .output()?
        .status
        .success()
    {
        return Ok("master".to_string());
    }

    color_eyre::eyre::bail!("Could not determine the base branch");
}

/// List the commits that are on the current branch but not on `base_branch`.
pub fn list_commits_on_this_branch(base_branch: &str) -> color_eyre::Result<Vec<(String, String)>> {
    let output = Command::new("git")
        .args([
            "log",
            "--oneline",
            "--no-decorate",
            "--end-of-options",
            &format!("{}..HEAD", base_branch),
        ])
        .output()?;

    if !output.status.success() {
        color_eyre::eyre::bail!(
            "Git command failed with status: {}, stderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let commit_ids = String::from_utf8(output.stdout)?
        .lines()
        .map(|line| {
            let mut parts = line.splitn(2, ' ');
            let commit_id = parts.next().unwrap().to_string();
            let message = parts.next().unwrap_or("").to_string();
            (commit_id, message)
        })
        .collect();

    Ok(commit_ids)
}

pub fn get_commit_diff(commit_id: &str) -> color_eyre::Result<String> {
    if !validate_commit_id(commit_id)? {
        color_eyre::eyre::bail!("Invalid commit ID: {}", commit_id);
    }

    let output = Command::new("git")
        .args(["show", "--no-color", "--end-of-options", commit_id])
        .output()?;

    if !output.status.success() {
        color_eyre::eyre::bail!("Git command failed with status: {}", output.status);
    }

    let diff = String::from_utf8(output.stdout)?;
    Ok(diff)
}

/// Get the diff of the current working tree against `base_ref`.
///
/// Uses `git diff <base_ref>` (not `<base_ref>..HEAD`), so the result includes
/// both commits on the current branch and uncommitted (staged + unstaged)
/// changes. `base_ref` may be a branch name (e.g. `origin/main`), a tag, or a
/// commit hash.
///
/// Note: no `--` separator is used because `git diff -- <rev>` treats `<rev>`
/// as a pathspec (yielding an empty diff); the revision must come before any
/// `--`. `base_ref` is trusted (sourced from `git symbolic-ref`, the CI env
/// var, or hardcoded `main`/`master`), so no disambiguation is needed.
pub fn get_branch_diff_against_base(base_ref: &str) -> color_eyre::Result<String> {
    let output = Command::new("git")
        .args(["diff", "--no-color", "--end-of-options", base_ref])
        .output()?;

    if !output.status.success() {
        color_eyre::eyre::bail!(
            "Git command failed with status: {}, stderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let diff = String::from_utf8(output.stdout)?;
    Ok(diff)
}

/// Validate that the given commit ID is just a valid commit ID, not an injection attempt.
pub fn validate_commit_id(commit_id: &str) -> color_eyre::Result<bool> {
    let possible_characters = "0123456789abcdefghijklmnopqrstuvwxyz";
    for c in commit_id.chars() {
        if !possible_characters.contains(c) {
            return Ok(false);
        }
    }
    Ok(true)
}
