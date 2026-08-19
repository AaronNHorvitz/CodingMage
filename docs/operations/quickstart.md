# Quickstart

## Initialize

Create private scratch and state locations and a new configuration path:

```bash
codingmage init --repo /absolute/project \
  --config /absolute/config/codingmage.toml \
  --scratch /absolute/private-worktrees \
  --state /absolute/private-state
```

`init` refuses relative paths, linked authority roots, and an existing configuration file. Its generated profile denies network, push, issues, and pull requests and keeps publication local.

## Inspect

```bash
codingmage doctor --config /absolute/config/codingmage.toml
codingmage plan --config /absolute/config/codingmage.toml
codingmage status --config /absolute/config/codingmage.toml
```

`doctor` authorizes and inventories the repository, parses the task plan, and emits redacted JSON. `plan` returns the first dependency-ready sub-task and immutable source hashes. `status` currently reports local repository and plan readiness. Live execution remains disabled.
