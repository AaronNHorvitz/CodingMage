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

`doctor` authorizes and inventories the repository, parses the task plan, and emits redacted JSON.
`plan` returns the first dependency-ready sub-task and immutable source hashes. `status` reports
local repository and plan readiness.

## Execute One Explicit Unit

After reviewing an absolute version 2 run specification:

```bash
codingmage run --config /absolute/config/codingmage.toml \
  --spec /absolute/config/run.toml
```

The supervised path retains successful work on a coordinator-owned local branch. It does not merge,
push, create a pull request, publish, or alter the active checkout.

## Execute A Serial Campaign

After reviewing an absolute version 2 campaign specification:

```bash
codingmage campaign --config /absolute/config/codingmage.toml \
  --campaign /absolute/config/campaign.toml
codingmage campaign-status --config /absolute/config/codingmage.toml \
  --campaign /absolute/config/campaign.toml
```

After independently verifying that an external prerequisite changed, clear only its exact blocker
with a fresh idempotency ID and a lowercase SHA-256 evidence digest:

```bash
codingmage campaign-clear-blocker \
  --config /absolute/config/codingmage.toml \
  --campaign /absolute/config/campaign.toml \
  --task 21.2.2.5 \
  --request clear-21-2-2-5-1 \
  --prerequisite-sha256 "${PREREQUISITE_SHA256}"
```

The command works only for the same local user and exact campaign, records a create-once
integrity-bound intent, revalidates the isolated campaign snapshot, and clears no other task. It
does not invoke a model or alter the active checkout.

Use one pod and `local_only` publication. The current campaign path is pre-release: interrupted
initial-implementation resume, expanded lead dispositions, production lifecycle controls, the prescribed
ten-outcome soak, and the controlled-target qualification gate remain incomplete. See
[`Serial Campaign`](serial-campaign.md) and
[`Unattended Safeguards`](../architecture/unattended-safeguards.md) before using valuable target
repositories.
