# Supervised One-Unit Run

## Boundary

`codingmage run` executes one operator-selected, dependency-ready sub-task. It refuses a dirty active checkout and never merges, pushes, publishes, opens a pull request, or changes the active checkout. Successful work remains on a local coordinator-owned feature branch for human inspection and integration.

The first production composition performs these operations in order:

1. Validate the project configuration and separate run specification.
2. Authorize and lock the exact repository by filesystem identity.
3. Parse the canonical task source and select the exact open dependency-ready sub-task.
4. Open a hash-chained journal and pin the packaged hidden process guard.
5. Create an owned feature branch and worktree from the exact active commit.
6. Probe the configured Claude and Codex CLI capability surfaces without authentication output.
7. Give Claude file-only authority inside the worktree, with Git, shell, web, MCP, skills, subagents, notebooks, and `.git` denied.
8. Verify Claude's claimed changed paths and create the implementation commit in the coordinator.
9. Execute configured shell-free deterministic gates through the bounded process guard.
10. Give Codex read-only authority over the exact base and candidate commits and validate its structured report against the immutable diff.
11. Repeat deterministic gates after a passing review.
12. Write a content-minimized idempotent checkpoint.
13. Change exactly the selected Markdown checkbox, validate that no other plan structure changed, and create a separate coordinator-owned completion commit.
14. Remove the clean owned worktree, release the repository lock, and retain the local feature branch.

`changes_required`, `disputed`, and `blocked` reviews stop as blocked in this first usable path. Automatic correction is not yet enabled because a corrected commit must receive another immutable senior review before it can be accepted.

## Run Spec

```toml
version = 1
task_id = "42.1.3.2"
owned_paths = ["kernel", "tests", "docs", "artifacts"]
completion_policy = "candidate_only"

[implementer]
executable = "/absolute/path/to/claude"
model = "opus"
effort = "high"
authentication = "existing_login"
maximum_budget_usd = "5.00"

[reviewer]
executable = "/absolute/path/to/codex"
model = "gpt-5.6-sol"
effort = "high"
```

The owned paths are authority, not suggestions. Every actual changed path must be reported by Claude and fall under one of them before the coordinator stages anything. `TASKS.md` is not delegated to Claude; CodingMage changes it mechanically only after gates and a passing exact-commit review.

Use `candidate_only` when a task has useful local work but still contains an unavailable external prerequisite. It stops in `checkpointed`, retains the reviewed implementation branch, and leaves the task checkbox open. Use `close_task` only when the complete task is achievable; any provider-reported limitation then fails closed before a completion claim.

## Invocation

```bash
cargo build --locked --release -p codingmage-cli
./target/release/codingmage doctor --config /absolute/codingmage.toml
./target/release/codingmage run --config /absolute/codingmage.toml --spec /absolute/run.toml
```

The terminal JSON identifies the run, task, terminal state, retained branch, candidate commit, completion commit, and review verdict. Provider output, prompts, source text, file names, paths, credentials, and hidden reasoning are not emitted.

`existing_login` passes only `HOME` and any present `XDG_RUNTIME_DIR`, `XDG_CONFIG_HOME`, and `DBUS_SESSION_BUS_ADDRESS` references to the provider processes. CodingMage adds the compiled literal `PATH=/usr/bin:/bin` so installed sandbox dependencies can be found; it never inherits ambient `PATH`. Login-discovery values remain in memory and are not emitted or journaled. API keys, tokens, and arbitrary configuration variables are never accepted through this boundary.

## Current Limits

- One live existing-login Claude implementation and exact-SHA Codex review are admitted as supervised local evidence; the five-unit and unattended campaigns remain open.
- `bare` Claude authentication has no production credential-helper composition yet.
- Automatic correction and repeat senior review are not implemented.
- The background service generator does not yet bind a run-spec queue to `codingmage run`.
- No branch is pushed or merged automatically.
- Crash recovery records state-changing uncertainty correctly, but production re-observation and resume of this concrete port remain open.
