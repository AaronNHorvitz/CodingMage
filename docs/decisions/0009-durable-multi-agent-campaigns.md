# Decision 0009: Durable Multi-Agent Campaigns

- **Status:** Accepted
- **Date:** 2026-08-21
- **Decision owners:** Repository owner and CodingMage maintainers
- **Related:** Decisions 0002, 0004, 0005, 0006, 0007, and 0008

## Context

The serial campaign runtime proves that one bounded implementation, verification, review,
correction, checkpoint, and integration sequence can execute without mutating the user's active
checkout. The campaign schema and lead response already permit multiple proposals, but the runtime
consumes one proposal and reconstructs its scheduler for each unit. Parallel subprocesses alone
would not provide durable ownership, fair scheduling, independent recovery, or safe integration.

CodingMage needs a production architecture that behaves like a small engineering team while keeping
all repository and publication authority deterministic.

## Decision

CodingMage will support a persistent hierarchical campaign with these roles:

- One read-only Codex team lead proposes a bounded batch from the coordinator-derived ready set.
- Up to the configured number of Claude implementation pods execute in isolated branches and
  worktrees. The supported initial configuration range is one through five; the schema may retain a
  higher hard limit for future qualification.
- A fresh Codex review session evaluates each immutable cumulative candidate diff.
- The same Claude session lineage receives bounded gate and review corrections when possible.
- One deterministic coordinator owns task state, leases, processes, commands, commits, GitHub
  writes, CI evidence, integration, and configured merge effects.

Serial mode remains supported and is the default. Parallel mode is an explicit campaign policy.

Each task receives one durable identity record joining its campaign, task, pod, branch, worktree,
base commit, candidate commit, issue, pull request, review sessions, implementation lineage, state,
limits, and evidence. Optional remote identities remain absent rather than synthesized when
publication is disabled.

Implementation pods may run concurrently; Git metadata mutation and campaign integration are
serialized. Every integration uses a durable intent, immutable reviewed candidate, current-head
compare-and-swap, isolated transfer or refresh, affected gates, and a fresh review whenever the
effective cumulative diff changes.

The default merge policy permits an eligible task to advance only the isolated campaign branch.
Promotion to the configured destination branch requires explicit human approval. A separately
configured automatic destination policy is valid only when every authorized task is terminally
complete, all deterministic and remote checks pass, final independent review passes, exact heads
remain unchanged, and repository protection permits the operation.

## Closed Policy Sets

Execution mode:

- `serial`
- `parallel`

Publication mode:

- `local_only`
- `per_task_draft_pull_request`
- `campaign_draft_pull_request`

Task integration policy:

- `never`
- `human_required`
- `auto_to_campaign_branch`

Destination promotion policy:

- `never`
- `human_required`
- `auto_to_default_branch`

No provider may modify these values. Monetary values are not authority inputs. Campaign and task
budgets use explicit provider-invocation, token, process, output, storage, elapsed-time, and
accepted-outcome units.

## Consequences

- Campaign checkpoints become larger and require a new fail-closed schema.
- A campaign-wide scheduler, integration queue, and GitHub writer each need one durable owner.
- Provider, test, GitHub, and integration concurrency limits are independent.
- Active pods must survive sibling failure and reconcile separately after restart.
- Stale-base candidates require controlled transfer and reverification rather than direct merging.
- Authenticated GitHub, native-platform, sustained-soak, independent-review, signing, and release
  claims remain external evidence.

## Rejected Alternatives

- Giving providers shell, Git, GitHub, or merge authority.
- Letting each pod merge its own work.
- Treating disjoint changed paths as proof of semantic independence.
- Replaying uncertain pushes, issue writes, pull-request writes, or merges without reconciliation.
- Creating work only to fill unused pod slots.
- Using a provider's prose as proof that a test, review, CI check, or merge passed.
