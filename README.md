<div align="center">

<h1>code-review-agent</h1>

<p>
  <b>An autonomous, LLM-powered code review agent written in Rust.</b>
</p>

<p>
  Diffs your branch against where it forked, hands the result to any OpenAI-compatible model,<br>
  and lets the agent explore your repository with <b>read-only</b> tools until it writes the review —<br>
  then posts it straight to your GitLab merge request.
</p>

<p>
  <a href="https://github.com/Stax124/code-review-agent/actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/Stax124/code-review-agent/ci.yml?branch=main&label=CI&logo=github&style=flat-square"></a>
  <a href="https://github.com/Stax124/code-review-agent/actions/workflows/build.yml"><img alt="Build" src="https://img.shields.io/github/actions/workflow/status/Stax124/code-review-agent/build.yml?label=build&logo=github&style=flat-square"></a>
  <a href="https://github.com/Stax124/code-review-agent/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/Stax124/code-review-agent?sort=semver&label=release&logo=github&style=flat-square"></a>
  <a href="https://github.com/Stax124/code-review-agent/releases"><img alt="Downloads" src="https://img.shields.io/github/downloads/Stax124/code-review-agent/total?label=downloads&logo=github&style=flat-square"></a>
</p>

<p>
  <img alt="Rust" src="https://img.shields.io/badge/rust-2024%20edition-CE422B?logo=rust&logoColor=white&style=flat-square">
  <img alt="Target" src="https://img.shields.io/badge/target-x86__64--unknown--linux--musl-4E9A06?logo=linux&logoColor=white&style=flat-square">
  <a href="https://github.com/Stax124/code-review-agent/stargazers"><img alt="Stars" src="https://img.shields.io/github/stars/Stax124/code-review-agent?style=flat-square&logo=github"></a>
</p>

<p>
  <a href="#quick-start"><b>Quick start</b></a> •
  <a href="#configuration"><b>Configuration</b></a> •
  <a href="#agent-tools"><b>Tools</b></a> •
  <a href="#publishing-to-gitlab"><b>GitLab CI</b></a> •
  <a href="#development"><b>Development</b></a>
</p>

</div>

---

Point it at a git repository and it will diff the current working tree against the branch it
diverged from, hand that diff to a model over any OpenAI-compatible chat-completions endpoint, and
let the model explore the repository with a small set of **read-only** tools until it produces a review.
When run inside a GitLab CI merge request pipeline it can post the finished review back as an MR
comment.

- **Single statically-linked binary** (`x86_64-unknown-linux-musl`), no runtime dependencies beyond `git`
- Provider-agnostic — anything speaking the OpenAI chat-completions API (OpenRouter by default)
- Streaming output with per-turn token/cost telemetry
- **Read-only, sandboxed tools**: the agent can never write to your repository
- Fully configurable via CLI flags, environment variables, or a `.env` file

---

## Table of contents

- [Table of contents](#table-of-contents)
- [How it works](#how-it-works)
- [Installation](#installation)
  - [Pre-built binary](#pre-built-binary)
  - [From source](#from-source)
- [Quick start](#quick-start)
- [Configuration](#configuration)
  - [Configuration precedence](#configuration-precedence)
  - [General](#general)
  - [Limits](#limits)
  - [Model options (might not be supported by all providers)](#model-options-might-not-be-supported-by-all-providers)
  - [System prompt](#system-prompt)
  - [Publishing](#publishing)
  - [Other environment variables](#other-environment-variables)
  - [CI environment variables](#ci-environment-variables)
  - [Built-in flags](#built-in-flags)
- [Customizing the system prompt](#customizing-the-system-prompt)
- [Agent tools](#agent-tools)
- [Base branch resolution](#base-branch-resolution)
- [Publishing to GitLab](#publishing-to-gitlab)
  - [Example `.gitlab-ci.yml`](#example-gitlab-ciyml)
- [Output](#output)
- [Non-configurable behavior](#non-configurable-behavior)
- [Development](#development)
  - [Layout](#layout)

---

## How it works

1. **Determine the base branch** the current branch diverged from (see
   [Base branch resolution](#base-branch-resolution)).
2. **Find the merge base** (`git merge-base <base> HEAD`) and compute the diff of the current
   working tree against it (`git diff <merge-base>`). Uncommitted changes are therefore included.
3. **Build the system prompt** by substituting the project structure, the commit list for the
   branch, and the full diff into the prompt template.
4. **Run the agent loop** for at most `--max-turns` turns. Each turn streams a completion (useful for debugging in CI envs); if the
   model requests tool calls they are executed and fed back, otherwise the loop ends and the last
   message is taken as the final review.
5. **Publish** the review as a GitLab MR note when a token and the required CI variables are
   present. Otherwise the review is only printed to stdout.

## Installation

### Pre-built binary

Download the `x86_64-unknown-linux-musl` binary from the
[releases page](https://github.com/Stax124/code-review-agent/releases/latest) and mark it executable:

```bash
chmod +x code-review-agent-x86_64-unknown-linux-musl
./code-review-agent-x86_64-unknown-linux-musl --help
```

### From source

Requires a stable Rust toolchain. The repository pins the `x86_64-unknown-linux-musl` target in
`rust-toolchain.toml` and `.cargo/config.toml`, so you also need `musl-tools` installed (or override
the target with `--target`).

```bash
cargo build --release
./target/x86_64-unknown-linux-musl/release/code-review-agent --help
```

## Quick start

```bash
cd /path/to/your/repo

code-review-agent \
  --api-key "$OPENROUTER_API_KEY" \
  --model "z-ai/glm-5.2" \
  --max-turns 20
```

Or with environment variables / a `.env` file in the repository you are reviewing:

```dotenv
CODE_REVIEW_AGENT_API_KEY=sk-or-v1-...
CODE_REVIEW_AGENT_MODEL=z-ai/glm-5.2
CODE_REVIEW_AGENT_MAX_TURNS=20
CODE_REVIEW_AGENT_REASONING_EFFORT=medium
```

```bash
code-review-agent
```

> The agent must be run from inside the git repository you want reviewed — the working directory is
> both the git repository root it diffs and the sandbox boundary for all tools.

---

## Configuration

Every option can be set as a CLI flag or as an environment variable. All agent-specific variables
are prefixed with `CODE_REVIEW_AGENT_`.

### Configuration precedence

1. **CLI flag** (highest priority)
2. **Environment variable**
3. **`.env` file** in the current working directory — loaded at startup via `dotenvy`; it does _not_
   override variables that are already set in the environment
4. **Built-in default** (lowest priority)

### General

| Flag                            | Environment variable             | Default                                         | Required | Description                                                                                                                                                                                                                                                          |
| ------------------------------- | -------------------------------- | ----------------------------------------------- | -------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--api-key <API_KEY>`           | `CODE_REVIEW_AGENT_API_KEY`      | —                                               | **yes**  | API key for the model provider. Sent as `Authorization: Bearer <key>`. The run aborts with an error if this is missing or empty. The value is hidden from `--help` output and redacted from debug logs.                                                              |
| `--api-endpoint <API_ENDPOINT>` | `CODE_REVIEW_AGENT_API_ENDPOINT` | `https://openrouter.ai/api/v1/chat/completions` | no       | Full URL of an OpenAI-compatible chat-completions endpoint. Must be a syntactically valid URL, or the run aborts. Point this at OpenAI, Azure OpenAI, a local vLLM/Ollama gateway, LiteLLM, etc. Note this is the **complete** endpoint path, not just the API base. |
| `--model <MODEL>`               | `CODE_REVIEW_AGENT_MODEL`        | `z-ai/glm-5.2`                                  | no       | Model identifier passed straight through in the request body as `model`. Use whatever naming your endpoint expects (e.g. `z-ai/glm-5.2` or `anthropic/claude-opus-5` for OpenRouter, `gpt-5` for OpenAI). Must not be empty.                                         |

### Limits

These bound how much work — and therefore how much money — a single run can consume.

| Flag                          | Environment variable            | Default | Description                                                                                                                                                                                                                                                                                                                                                                                                         |
| ----------------------------- | ------------------------------- | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--max-turns <MAX_TURNS>`     | `CODE_REVIEW_AGENT_MAX_TURNS`   | `20`    | Hard cap on agent turns (one turn = one completion request plus any tool calls it triggers). When the cap is reached the run stops and logs a warning; if the agent had not produced a final answer, no review is published. An empty value falls back to `20`; a non-numeric value is a hard error. Larger values allow deeper exploration of the codebase at higher cost.                                         |
| `--max-retries <MAX_RETRIES>` | `CODE_REVIEW_AGENT_MAX_RETRIES` | `3`     | Maximum retries for a failed provider request **within a single turn**. The original attempt is not counted, so `3` means up to 4 attempts. `0` disables retries. Only HTTP `429 Too Many Requests` is retried — any other failure aborts immediately. Retries use exponential backoff starting at 2 s and capped at 30 s, and do not advance the conversation or consume a turn. An empty value falls back to `3`. |

### Model options (might not be supported by all providers)

Optional, provider-specific fields that are merged into the JSON request body. They are omitted
entirely when unset, so a provider that does not understand them never sees them.

| Flag                                              | Environment variable                      | Default | Request body field               | Description                                                                                                                                                                                                  |
| ------------------------------------------------- | ----------------------------------------- | ------- | -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `--reasoning-effort <REASONING_EFFORT>`           | `CODE_REVIEW_AGENT_REASONING_EFFORT`      | unset   | `reasoning: { effort: <value> }` | Reasoning/thinking effort for reasoning-capable models. Typically `low`, `medium` or `high`, but any model-specific string is passed through verbatim. Empty or whitespace-only values are treated as unset. |
| `--max-completion-tokens <MAX_COMPLETION_TOKENS>` | `CODE_REVIEW_AGENT_MAX_COMPLETION_TOKENS` | unset   | `max_completion_tokens`          | Upper bound on tokens the model may generate per response. Leave unset to use the provider default. Setting this too low can truncate a long review.                                                         |

### System prompt

| Flag                                        | Environment variable                   | Default                        | Description                                                                                                                                                                                                                                                                                                                                                                                                          |
| ------------------------------------------- | -------------------------------------- | ------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--system-prompt-path <SYSTEM_PROMPT_PATH>` | `CODE_REVIEW_AGENT_SYSTEM_PROMPT_PATH` | unset → `CODE_REVIEW_AGENT.md` | Path to a file containing a custom system prompt template. When unset, `CODE_REVIEW_AGENT.md` in the current directory is used if it exists. If neither is available (or the file is empty, not a regular file, unreadable, or resolves outside the current directory), the built-in default prompt is used and the reason is logged at `INFO`. See [Customizing the system prompt](#customizing-the-system-prompt). |

### Publishing

| Flag                            | Environment variable                                                 | Default | Description                                                                                                                                                                                                                                                                                                                   |
| ------------------------------- | -------------------------------------------------------------------- | ------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--gitlab-token <GITLAB_TOKEN>` | `CODE_REVIEW_AGENT_GITLAB_TOKEN`, falling back to `GITLAB_API_TOKEN` | unset   | GitLab personal/project/job access token with `api` scope, used to post the review as a merge request note (`PRIVATE-TOKEN` header). When absent or empty, publishing is disabled and the review is only printed. Publishing additionally requires the CI variables below. Hidden from `--help` and redacted from debug logs. |

### Other environment variables

| Variable                     | Default | Description                                                                                                                                                                             |
| ---------------------------- | ------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `RUST_LOG`                   | `info`  | Standard `tracing-subscriber` env filter controlling log verbosity, e.g. `debug`, `code_review_agent=debug`, `warn`. Set to `debug` to log the full raw response of every turn.         |
| `GITLAB_API_TOKEN`           | unset   | Alternative name for the GitLab token; used only when `--gitlab-token` / `CODE_REVIEW_AGENT_GITLAB_TOKEN` is unset or empty. Convenient in pipelines that already define this variable. |
| `NO_COLOR` / `CLICOLOR` etc. | —       | Honoured by the `colored` crate and by clap's `auto` color choice; color is disabled automatically when stdout is not a TTY.                                                            |

### CI environment variables

These are read directly from the environment (there are no equivalent flags) and are set
automatically by GitLab CI.

| Variable                              | Used for    | Description                                                                                                                                                         |
| ------------------------------------- | ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `CI_MERGE_REQUEST_TARGET_BRANCH_NAME` | base branch | Highest-priority source for the branch to diff against.                                                                                                             |
| `CI_PROJECT_ID`                       | publishing  | Project the MR note is posted to. **Required** for publishing.                                                                                                      |
| `CI_MERGE_REQUEST_IID`                | publishing  | Merge request IID the note is posted to.                                                                                                                            |
| `CI_OPEN_MERGE_REQUESTS`              | publishing  | Fallback when `CI_MERGE_REQUEST_IID` is unset (i.e. on a branch pipeline rather than an MR pipeline). The first entry is parsed, e.g. `group/project!42` → MR `42`. |
| `CI_API_V4_URL`                       | publishing  | GitLab API base URL. Defaults to `https://gitlab.com/api/v4`; set automatically on self-hosted instances. Trailing slashes are stripped.                            |

If `CI_PROJECT_ID` or both MR-IID variables are missing, publishing is skipped with a warning even
when a token is present.

### Built-in flags

| Flag              | Description                                                                                                                |
| ----------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `-h`, `--help`    | Print help, grouped into the _General_, _Safety_, _Model options_, _System prompt_ and _Publishing_ sections listed above. |
| `-V`, `--version` | Print the binary version.                                                                                                  |

---

## Customizing the system prompt

Create a `CODE_REVIEW_AGENT.md` file in the repository being reviewed, or point
`--system-prompt-path` at any file **inside the current working directory**. The following
placeholders are substituted before the prompt is sent:

| Placeholder                | Replaced with                                                                                                 |
| -------------------------- | ------------------------------------------------------------------------------------------------------------- |
| `{project_structure}`      | Non-recursive listing of the current directory (files with sizes, directories with a trailing slash).         |
| `{commits_on_this_branch}` | `<short-sha> <subject>` for every commit on the current branch but not on the base branch, most recent first. |
| `{full_diff}`              | The full working-tree diff against the merge base.                                                            |

Placeholders are optional — a prompt that omits `{full_diff}` simply will not contain the diff. The
built-in default prompt (see `src/constants.rs`) is a good starting point: it defines the review
criteria, a severity scale (🔴 Critical / 🟠 High / 🟡 Medium / 🟢 Low), a GitLab-Markdown-friendly
output format, and a prompt-injection guard instructing the model to ignore instructions found
inside the diff.

**Safety constraints** on the prompt file — violating any of these silently falls back to the
default prompt (logged at `INFO`):

- the path must exist and be a regular file (not a directory)
- it must not be empty
- after resolving symlinks it must still be inside the current working directory

---

## Agent tools

All tools are read-only and confined to the current working directory. Path traversal outside the
working directory is rejected, and `.gitignore`d files are excluded.

| Tool                  | Parameters                                                   | Description                                                                                                                                                                        |
| --------------------- | ------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `read_file`           | `file` (required), `start_line`, `end_line`                  | Reads a whole file or a 1-based inclusive line range. Each line is prefixed with its line number. Refuses `.gitignore`d files and symlinks pointing outside the working directory. |
| `list_directory`      | `subdirectory`                                               | Non-recursive listing of a directory; files show their size, directories a trailing slash.                                                                                         |
| `file_tree`           | `subdirectory`                                               | Recursive tree honouring `.gitignore` and other ignore files. Truncated at 10 levels of depth, 5,000 entries, or 1 MB of output — whichever comes first.                           |
| `search_in_directory` | `search_pattern` (required), `subdirectory`, `context_lines` | Regex search across the directory, with optional lines of context around each match (default `0`).                                                                                 |
| `get_commit_diff`     | `commit_id` (required)                                       | `git show` for a single commit. The commit ID is validated against an alphanumeric allow-list to prevent argument injection.                                                       |

---

## Base branch resolution

The base branch is resolved in this order, first match wins:

1. `CI_MERGE_REQUEST_TARGET_BRANCH_NAME` (GitLab CI merge request pipelines)
2. The remote's default branch via `git symbolic-ref --short refs/remotes/origin/HEAD` (e.g.
   `origin/main`) — this lets the agent run usefully even directly on the default branch, reviewing
   local commits ahead of the remote plus uncommitted changes
3. A local `main` branch
4. A local `master` branch

If none resolve, the run aborts with `Could not determine the base branch`.

---

## Publishing to GitLab

Publishing happens only when **all** of the following hold:

- a token is available (`--gitlab-token`, `CODE_REVIEW_AGENT_GITLAB_TOKEN` or `GITLAB_API_TOKEN`)
- `CI_PROJECT_ID` is set
- `CI_MERGE_REQUEST_IID` is set, or an MR IID can be parsed from `CI_OPEN_MERGE_REQUESTS`
- the agent actually produced a non-empty final review

The review is posted to `POST {CI_API_V4_URL}/projects/{id}/merge_requests/{iid}/notes` with a
telemetry footer appended listing per-provider token usage and cost. A failure to publish is logged
as an error but does not fail the run.

### Example `.gitlab-ci.yml`

```yaml
code_review:
  stage: test
  image: alpine:latest
  rules:
    - if: $CI_PIPELINE_SOURCE == "merge_request_event"
  variables:
    GIT_DEPTH: 0 # full history is required for `git merge-base`
    CODE_REVIEW_AGENT_MODEL: "z-ai/glm-5.2"
    CODE_REVIEW_AGENT_MAX_TURNS: "20"
    CODE_REVIEW_AGENT_REASONING_EFFORT: "medium"
  before_script:
    - apk add --no-cache git curl
    - curl -sSLo /usr/local/bin/code-review-agent "$AGENT_BINARY_URL"
    - chmod +x /usr/local/bin/code-review-agent
  script:
    - code-review-agent
  # CODE_REVIEW_AGENT_API_KEY and CODE_REVIEW_AGENT_GITLAB_TOKEN are set as
  # masked, protected CI/CD variables in the project settings.
```

`GIT_DEPTH: 0` matters: a shallow clone has no common ancestor with the target branch, so
`git merge-base` fails.

---

## Output

Each turn prints a header, the streamed model response, the tool calls it made, and per-turn usage:

```
── Turn 3/20 ──
Tool calls (2):
- read_file: src/config.rs
- search_in_directory: max_turns
Summary: Prompt tokens: 24.13k, Completion tokens: 1.21k, Total tokens: 25.34k, Cost: $0.014820
Routed to: Z.ai
```

The run ends with an aggregated per-provider telemetry summary.

---

## Non-configurable behavior

Values that are currently hard-coded, for reference when tuning:

| Behavior                   | Value                                                |
| -------------------------- | ---------------------------------------------------- |
| Sampling temperature       | `0.0`                                                |
| Streaming                  | always on (`stream: true`, with `include_usage`)     |
| Turn reminders injected at | 15, 10 and 5 turns remaining, plus a final-turn note |
| Retry backoff              | 2 s initial, doubling, capped at 30 s                |
| Retried status codes       | `429 Too Many Requests` only                         |
| Provider HTTP timeouts     | 30 s connect, 120 s read                             |
| GitLab HTTP timeouts       | 5 s connect, 20 s read                               |
| Max tool calls per turn    | 128                                                  |
| Max stream line length     | 1 MiB                                                |
| `file_tree` limits         | 10 levels, 5,000 entries, 1 MB output                |

---

## Development

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

CI runs `fmt`, `clippy` and a release build for `x86_64-unknown-linux-musl` on every push and pull
request. Tagged pushes additionally verify that the tag matches the version in `Cargo.toml` and
publish the binary to a GitHub release.

### Layout

```
src/
├── main.rs                  # agent loop, retries, publishing
├── config.rs                # CLI/env configuration and validation
├── constants.rs             # default system prompt
├── stream.rs                # streaming output printer
├── agent/
│   ├── client.rs            # OpenAI-compatible client, SSE parsing, tool dispatch
│   ├── response.rs          # response/stream types
│   └── telemetry.rs         # per-provider token and cost accounting
├── publisher/
│   └── gitlab.rs            # GitLab MR note publisher
├── tools/                   # read_file, list_directory, file_tree, search, commit diff
└── utils/                   # git helpers, directory listing, prompt assembly, logging
```
