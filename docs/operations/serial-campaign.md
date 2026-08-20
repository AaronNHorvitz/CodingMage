# Serial Campaign

## Boundary

`codingmage campaign` is the first live hierarchical-campaign rollout. It processes one pod at a
time from an isolated evolving campaign branch. It never modifies the active checkout, pushes a
branch, creates or approves a pull request, merges a GitHub pull request, publishes a release, or
changes a protected branch.

Each accepted unit follows this sequence:

1. Reparse the canonical task source from the exact current campaign head.
2. Compute a deterministic dependency-ready set and expose one task to the read-only Codex lead.
3. Validate the structured proposal against the exact head, source digest, dependencies, paths,
   gate tiers, resources, artifacts, and campaign authority.
4. Run the production Claude implementation, local gates, bounded correction, Codex review, final
   verification, checkpoint, and mechanical task-completion workflow.
5. Preview the complete changed-path set from the old campaign head to the reviewed completion.
6. Fast-forward only the coordinator-owned campaign branch when ancestry, paths, identity, and
   cleanliness still match.
7. Repeat from the new exact head until completion, the unit ceiling, or a truthful blocker.

## Campaign Spec

The values below are illustrative. Repository identity, commit, source digest, paths, executables,
models, budgets, and branch policy must be selected from the intended target and local installation.

```toml
version = 1
campaign_id = "example-campaign"
repository_id = "repo-example"
repository_path = "/absolute/repository"
initial_commit = "0123456789abcdef0123456789abcdef01234567"
task_source_sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
operator_authorization_sha256 = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
max_parallel_pods = 1
max_units = 10
maximum_budget_usd = "50.00"
implementer_authentication = "existing_login"
maximum_invocation_budget_usd = "5.00"
campaign_branch = "codingmage/example-campaign"
allowed_paths = ["crates", "docs", "tests"]
denied_paths = ["private"]
protected_branches = ["main"]
publication = "local_only"

[team_lead]
executable = "/absolute/path/to/codex"
model = "gpt-5.6-sol"
effort = "high"

[implementer]
executable = "/absolute/path/to/claude"
model = "opus"
effort = "high"

[reviewer]
executable = "/absolute/path/to/codex"
model = "gpt-5.6-sol"
effort = "high"

[[gate_tiers]]
name = "focused"
profiles = ["configured-gates"]
```

Allowed and denied roots must be disjoint repository-relative paths. Gate profile names identify
operator-authored policy; a model cannot supply shell commands. Omitted `max_parallel_pods`
defaults to one, and this rollout still admits only one proposal per generation when a higher ceiling
is explicitly present.

## Invocation

```bash
cargo build --locked --release -p codingmage-cli
./target/release/codingmage doctor --config /absolute/codingmage.toml
./target/release/codingmage campaign \
  --config /absolute/codingmage.toml \
  --campaign /absolute/campaign.toml
```

The final JSON reports campaign identity, terminal state, retained local branch, exact head,
completed-unit count, last task, and a content-free blocker code when applicable. Live stderr adds
`codex-lead` and `integration` stages to the existing per-unit activity stream.

## Current Limits

- Campaign-level durable journaling and same-campaign restart recovery are not complete.
- Provider quota pause/resume and operator stop-after-unit controls are not complete.
- Aggregate observed provider cost is not yet reconciled; configured unit and per-invocation
  ceilings remain the enforceable bounds.
- Parallel live pods remain disabled even if the authority ceiling is greater than one.
- Story-level draft PR publication and authenticated GitHub campaign evidence remain open.
- A retained campaign branch requires human inspection; protected/default-branch promotion remains
  outside campaign authority.
