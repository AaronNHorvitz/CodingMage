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

## Execute A Campaign

After reviewing an absolute version 2 campaign specification:

```bash
codingmage campaign --config /absolute/config/codingmage.toml \
  --campaign /absolute/config/campaign.toml
codingmage campaign-status --config /absolute/config/codingmage.toml \
  --campaign /absolute/config/campaign.toml
codingmage campaign-report --config /absolute/config/codingmage.toml \
  --campaign /absolute/config/campaign.toml
codingmage campaign-explain-blocker --config /absolute/config/codingmage.toml \
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

After independently verifying an external deferral trigger, return only that exact task to ready-set
evaluation with a fresh request ID and evidence digest:

```bash
codingmage campaign-observe-trigger \
  --config /absolute/config/codingmage.toml \
  --campaign /absolute/config/campaign.toml \
  --task 21.2.3.3 \
  --trigger operator_resume \
  --request resume-21-2-3-3-1 \
  --evidence-sha256 "${TRIGGER_EVIDENCE_SHA256}"
```

Only `provider_reset`, `review_completion`, and `operator_resume` are external controls. Campaign-head
advancement, lease release, and gate-resource release are observed automatically from
coordinator-owned state. Exact request replay is idempotent; conflicting request reuse fails closed.

Omitting `multi_agent` preserves serial compatibility. An explicit parallel policy may configure one
through five implementers, separate reviewer and test-worker limits, `local_only` or exact remote
publication, one serialized integration worker, and human-required destination promotion. See
[`Configuration`](configuration.md) and
[`Durable Multi-Agent Campaign Architecture`](../architecture/multi-agent-campaigns.md).

The current campaign path remains pre-release. The prescribed production ten-outcome soak,
human-reconciled controlled-target campaign, authenticated live providers and GitHub,
long-duration or live-provider five-pod execution, native platforms, independent review, signing,
and publication remain open. The guarded two-cycle local five-pod qualification has passed; it does
not satisfy those broader gates. See
[`Unattended Safeguards`](../architecture/unattended-safeguards.md) before using valuable target
repositories.

## Run Guarded Qualification

Ordinary tests cannot invoke real providers or GitHub accidentally. For an explicitly authorized
target, first review the exact configuration, campaign, and independent authorization record. The
first invocation runs `doctor` and source-free provider capability probes, writes a private report,
prints its SHA-256 digest, and stops before model inference:

```bash
python3 scripts/qualify_campaign.py \
  --mode providers \
  --binary /absolute/codingmage \
  --config /absolute/config.toml \
  --campaign /absolute/campaign.toml \
  --authorization /absolute/operator-authorization.txt \
  --preflight-report /absolute/private/preflight.json
```

Manually inspect that report. If it matches the reviewed authority, repeat the command with the
printed digest and the external-effects acknowledgment:

```bash
CODINGMAGE_LIVE_QUALIFICATION=I_ACKNOWLEDGE_EXTERNAL_EFFECTS \
  python3 scripts/qualify_campaign.py \
  --mode providers \
  --binary /absolute/codingmage \
  --config /absolute/config.toml \
  --campaign /absolute/campaign.toml \
  --authorization /absolute/operator-authorization.txt \
  --preflight-report /absolute/private/preflight.json \
  --approved-preflight-sha256 PRINTED_SHA256
```

The controlled-target `providers` mode requires one pod, exactly ten accepted outcomes, local-only
publication, a dedicated clean branch, and denied external capabilities. `github` and `full` require
separate exact remote authority. Every mode refuses automatic destination promotion, linked or
relative selected files, a dirty or moved target head, changed preflight bytes, missing approval,
missing acknowledgment, an authorization record or report inside the target repository, or
inconsistent publication policy. Provider probes use only version/help surfaces and existing login
stores; they do not invoke model inference or read credential values.
