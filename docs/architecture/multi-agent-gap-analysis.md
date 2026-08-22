# Multi-Agent Campaign Implementation Reconciliation

## Baseline

- Repository branch: `feat/hierarchical-campaigns`
- Reconciliation date: 2026-08-21
- Scope: implementation, deterministic local evidence, operator boundaries, and remaining external
  qualification

This is a non-normative implementation aid. `README.md`, `TASKS.md`, accepted decisions, typed
source contracts, and current evidence records remain authoritative.

## Implemented Locally

| Area | Current behavior | Primary implementation |
| --- | --- | --- |
| Planning | One read-only Codex lead can return a bounded multi-proposal batch or one closed blocker, deferral, or human-decision disposition. Every proposal is revalidated against one immutable generation. | `codingmage-campaign`, `codingmage-runtime::team_planning` |
| Scheduler | A campaign-lifetime scheduler persists stable sequence, leases, actor and physical resources, provider circuits, and task records. It admits only dependency-ready nonconflicting work. | `codingmage-campaign::team` |
| Execution | One through five implementation workers run concurrently with task-scoped identities, cancellation, timeout, panic isolation, ordered collection, durable liveness, and sibling preservation. | `codingmage-runtime::team_runtime` |
| Gates and review | Shared gate and reviewer semaphores bound execution. Every candidate uses deterministic gates and fresh cumulative Codex review; accepted findings return only to the matching lineage. | `codingmage-runtime::team_runtime` |
| Task state | Closed states cover planning through publication, CI, integration, terminal noncompletion, and merge. Integrity-bound snapshots and events reject stale, skipped, duplicate, and cross-task transitions. | `codingmage-campaign::team` |
| Publication | Assigned issues are synchronized before provider execution when remote mode is enabled. Exact reviewed branches, task draft PRs, commit-bound CI, correction, and completion use idempotent durable mappings. | `codingmage-runtime::team_publication`, `codingmage-runtime::team_github` |
| Integration | One durable queue serializes accepted candidates. Exact fast-forward or isolated squash transfer, compare-and-swap head advancement, cumulative validation, and crash reconciliation preserve candidates on refusal. | `codingmage-runtime::team_integration`, `codingmage-git::integration` |
| Promotion | Final gates and review produce a content-minimized report and optional final draft PR. Task and destination grants are separate, exact, idempotent, and human-gated by default. | `codingmage-runtime::team_promotion`, `codingmage-runtime::team_control` |
| Recovery | Initial batches, lifecycle events, CI corrections, publication, integration, and promotion reobserve durable intent and actual state before resuming. | `codingmage-runtime` team modules |
| Monitoring | Status projects active pods, lifecycle, queue, resource use, terminal reasons, and final report state without source or provider prose. Same-user controls are idempotent. | `codingmage-runtime::team_control`, `codingmage-cli` |
| Qualification | Deterministic capacities one through five, a process-backed five-pod workflow, failure permutations, guarded sustained soak, and guarded live qualification exist. | `codingmage-runtime`, `codingmage-cli/tests/workflow.rs`, `scripts/qualify_campaign.py` |

The required 44-scenario mapping is machine checked in
[`multi-agent-scenario-matrix.json`](../evidence/multi-agent-scenario-matrix.json).

## Remaining Evidence Gaps

| Boundary | Why it remains open | Required evidence |
| --- | --- | --- |
| Prescribed serial production qualification | Gate 22.3 has not been executed after the latest implementation change. | Disposable ten-outcome run and human-reconciled ten-task controlled target. |
| Real-provider parallel execution | Ordinary tests deliberately cannot use ambient authenticated providers. | Guarded disposable campaign with approved provider logins and retained content-minimized report. |
| Authenticated GitHub and CI | Fake transport proves policy and idempotency, not service identity or live branch protection. | Guarded disposable repository covering issue, push, draft PR, CI, correction, and integration. |
| Sustained duration | An ignored bounded harness exists but no current post-correction result is recorded. | Run the guarded five-pod soak and record duration, cycles, resource growth, residue, and source commit. |
| Native platforms | Current evidence is Linux-local. | Native macOS and Windows process, filesystem, credential, packaging, and lifecycle runs. |
| Independent judgment | Automated model review is not independent human approval. | Human security and architecture review with resolved findings. |
| Release effects | Signing, tagging, publishing, and post-download verification are intentionally outside implementation tests. | Separately authorized release-candidate procedure. |

## Intentionally Bounded Behavior

- Serial mode remains the compatibility default when `multi_agent` is absent.
- Arbitrary semantic coupling cannot be proven mechanically. Operators declare shared contracts and
  resources; cumulative gates, Codex review, or a human decision cover residual judgment.
- Follow-up work is count-bounded and must remain inside original authority. Model output cannot
  expand scope.
- Remote publication is optional. Local state stays canonical and remote text never grants
  authority.
- Automatic task integration and destination promotion are different permissions. Destination
  promotion is human-required by default.

## Rollout Truth

Implemented local behavior is not synonymous with qualified valuable-target operation. A second
live pod, authenticated mutation, sustained unattended campaign, native-platform claim, signed
package, or public release must remain described as unqualified until its explicit evidence gate
passes after the last relevant implementation correction.
