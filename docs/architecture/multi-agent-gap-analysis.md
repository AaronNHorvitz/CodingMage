# Multi-Agent Campaign Gap Analysis

## Audit Baseline

- Repository branch: `feat/hierarchical-campaigns`
- Audited source baseline: `b13973f`
- Audit date: 2026-08-21
- Scope: local implementation, deterministic fakes, documentation, and qualification boundaries

This analysis is an internal implementation aid. `README.md`, `TASKS.md`, accepted architecture
decisions, and typed source contracts remain authoritative.

## Working Foundations

CodingMage already has the following foundations and they must be preserved:

- A deny-first target authorization model with exact repository, path, branch, and capability
  boundaries.
- A bounded one-unit runtime that composes a Claude implementation session, coordinator-owned Git
  commit, deterministic gates, an independent Codex review, bounded correction, checkpointing,
  task reconciliation, and exact resource release.
- A serial campaign runtime that reparses the canonical plan from an evolving isolated campaign
  head and handles completion, blockers, deferrals, human-decision holds, provider pauses, limits,
  controls, and restart recovery.
- Structured Codex team-lead and reviewer adapters whose output is untrusted data.
- A `PodScheduler` that rejects overlapping path and test-resource leases and enforces a configured
  capacity.
- Owned worktrees and branches with physical-identity manifests, hostile Git configuration checks,
  coordinator-owned commits, immutable review ranges, and exact fast-forward integration.
- Integrity-bound campaign checkpoints, a hash-chained journal projection, idempotent operator
  controls, content-minimized status, and fail-closed legacy-state refusal.
- A deny-first GitHub core with exact identity checks, marker-bounded human-content preservation,
  draft-only pull-request content, idempotency keys, optimistic concurrency, and uncertain-write
  reconciliation.
- Deterministic fake providers, repository fixtures, GitHub transport fixtures, disposable soak
  fixtures, Linux packaging tools, and local policy gates.

## Material Gaps

| Area | Current behavior | Required change |
| --- | --- | --- |
| Planning | Lead schema can return multiple proposals. | Preserve an ordered proposal batch and validate every assignment against one immutable generation. |
| Scheduler lifetime | `PodScheduler` is recreated for each serial unit. | Persist scheduler sequence, active leases, queue age, capacity, heartbeats, and terminal lease disposition for the campaign lifetime. |
| Admission | Only the first validated proposal is consumed. | Admit every dependency-ready, nonconflicting proposal up to configured actor and resource ceilings. |
| Unit lock | The one-unit runtime takes a repository-wide lock beneath its state root. | Retain one campaign lease and use exact task/pod claims so independent pods can execute concurrently. |
| Execution | Campaign code invokes one unit synchronously. | Run bounded pod workers concurrently, isolate cancellation and failure, and collect terminal results without cancelling healthy siblings. |
| Gate scheduling | Gate execution is bounded inside one unit. | Add campaign-level test-worker and named-resource admission so conflicting gates serialize while independent gates may run concurrently. |
| Durable task state | The one-unit state machine stops at local completion. | Add campaign task states for publication, CI, integration, merge, dispute, cancellation, and recovery. |
| Identity mapping | Run, task, worktree, and branch identity exist in separate records. | Persist one integrity-bound task record joining campaign, task, issue, pod, worktree, branch, base, candidate, PR, and provider sessions. |
| Integration | Integration requires the reviewed commit to descend from the current campaign head. | Add immutable integration intents, stale-base detection, patch-transfer or refresh into an isolated integration worktree, affected gates, and compare-and-swap head advancement. |
| GitHub publication | Core issue and draft-PR primitives exist but are not composed into campaign runtime. | Add task-owned issue/PR sections, exact branch push, uncertain-write reconciliation, CI status ingestion, and idempotent resume. |
| Merge policy | Protected-branch mutation is absent. | Add a closed policy enum with deny-first defaults and deterministic preconditions; keep default-branch promotion human-gated by default. |
| Recovery | Serial head and correction recovery are implemented; initial implementation replay remains blocked. | Reconcile every active pod and external write independently, detect stale heartbeats, and never replay an uncertain non-idempotent effect. |
| Monitoring | Serial status and controls are implemented. | Project multiple active pods, queue and integration state, resource utilization, and per-task terminal codes without exposing repository or provider content. |
| Qualification | Serial and bounded unattended pilots exist. | Add deterministic five-pod, publication, CI, integration-order, recovery, merge-policy, and sustained-soak campaigns plus opt-in credential-gated live qualification. |

## Security Consequences

Concurrency must not broaden model authority. The coordinator remains the sole holder of Git,
GitHub, test-command, state-transition, integration, and merge authority. Each pod receives one
immutable task packet and one exact path lease. Provider output cannot create a lease, command,
issue, pull request, commit, integration operation, policy change, or completion transition.

The campaign state must fail closed when any identity is missing, duplicated, stale, contradictory,
or bound to another generation. Shared-resource conflicts include equal paths, ancestors,
descendants, rename sources, generated artifacts, public contracts, schemas, migrations, ports,
databases, devices, services, and operator-declared resources.

## Implementation Boundary

The change should extend existing contracts rather than introduce a second coordinator:

1. Extend `codingmage-campaign` with closed execution, publication, integration, merge, task-state,
   resource, and durable pod-record contracts.
2. Extend `codingmage-runtime` with an integrity-bound multi-pod checkpoint and one persistent
   scheduler that composes the existing one-unit implementation and review path.
3. Extend `codingmage-git` with a mutation-free integration preview and isolated stale-base
   transfer primitive.
4. Extend `codingmage-github` with per-task records, issue/PR owned sections, CI projections, and
   merge-policy authorization over an abstract transport.
5. Keep `codingmage campaign` serial by default. Parallel execution requires explicit authority and
   remains locally publish-disabled unless the corresponding GitHub capabilities are also granted.
6. Keep live provider and GitHub qualification opt-in, credential-gated, and excluded from ordinary
   tests.

## Rollout Truth

Architecture and deterministic fake-backed behavior may be implemented before production
qualification, but a second live pod, authenticated GitHub mutation, default-branch merge, package
publication, or public release must not be described as qualified until its explicit gate passes.
