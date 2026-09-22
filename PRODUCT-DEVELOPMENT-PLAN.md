# CodingMage Product Development Plan

## Status and Ownership

This is the delivery guide for [PRD.md](PRD.md), under
[Decision 0014](docs/decisions/0014-owner-optional-engineering-team.md).
[TASKS.md](TASKS.md) remains the sole canonical implementation/evidence ledger. This document
introduces no parallel task statuses, qualified runtime flags or release approval.

The goal is a complete engineering campaign with an optional hands-off product owner, not a larger
collection of loosely supervised terminal sessions. Preserve the existing coordinator, isolated
worktrees, immutable review, deterministic verification, durable journal and integration queue.

## Existing Assets and Required Reconciliation

| Area | Reuse | Work to establish |
| --- | --- | --- |
| Readiness and decomposition | Existing census, authority envelopes and sealed child packets | Map mission criteria and delegated decisions without widening task authority. |
| Pods and scheduling | Existing durable scheduler, worker pool, leases and circuits | Add director/team roles and interface-resource dependencies without a second scheduler. |
| Review and integration | Existing exact-commit review, corrections and serialized integration | Preserve independence across new roles and evaluate integrated product behavior. |
| State and controls | Existing journals, checkpoints, task projections and operator controls | Add involvement/decision history, expiry/revocation and noninteractive holds. |
| Native Linux workspace | Existing CLI/service contracts, campaign status, controls and reports | Add a usable desktop presentation and guided setup without duplicating coordinator authority. |
| Providers | Existing qualified boundaries and fake adapters | Qualify each newly configured role/model combination and its actual confinement. |
| Evidence | Existing tests, campaigns and retained failures | Reconcile current-source bindings and execute the new no-intervention matrix. |

The published architecture, historical reports and current ledger do not always describe the same
source revision. First reconcile exact code, gate requirements and evidence. The known
Task 25.2.4.6 freshness gap remains open; new planning does not repair or reset it. Development use
of an external coding tool is not evidence for a supported runtime adapter.

## Delivery Sequence

| Stage | Tasks | Exit evidence |
| --- | --- | --- |
| Existing foundation | Necessary existing gates, especially current-source renewal and one-pod live boundaries | Exact reusable source, roles, checks and remaining external blockers are identified. |
| Native Linux UI | Sprint 36, independently of unimplemented team/stack extensions | A real local workspace and campaign workflow over available backend capabilities, with honest qualification limits. |
| Mission and involvement | Sprint 32 | Closed contracts, mode matrix, no-prompt dispositions, revocation and restart refusal tests. |
| Engineering team | Sprint 33 | Director/lead/specialist packets validated through the existing coordinator with independent review. |
| Integrated delivery | Sprint 34 | Interface-aware composition, real product acceptance and durable source-backed handoffs. |
| Hands-off local | Story 35.1 and Task 35.2.2 | One live pod, then qualified staged concurrency, no owner responses and complete outcome reconciliation. |
| Optional remote delivery | Task 35.2.1 | Exact preauthorized remote effects and stale/uncertain promotion behavior on a disposable remote. |

Sprints 32 through 35 may develop local contracts, fakes and consumer tests after their exact local
prerequisites are available. They do not require optional host, memory or Muse integration, public
signing or publication merely to develop locally. Record earlier unavailable prerequisites before
using this continuation; do not abandon an unblocked foundational defect. Actual live execution,
parallelism and external effects still require their existing qualification gates.

## Native UI Architecture and Agent Brief

This is an implementation assignment for a future agent, not authorization for this documentation
pass to write code, add dependencies, build binaries or launch agents. The owner chose a native
Linux desktop app. Keep the development process simple: one designated writer on the selected
development branch, preserving existing work and fixing forward. Do not create another planning
branch, reviewer workspace or parallel development agent merely to deliver the UI. This does not
remove the runtime's isolated target worktrees or required independent review.

The architecture is native presentation -> existing command/service boundary -> deterministic
coordinator -> existing providers, isolated worktrees, gates and durable state. Reuse repository
discovery, task parsing, configuration validation, preflight, campaign controls and reports.
Prefer an established Rust-compatible native toolkit; record the choice against licensing,
accessibility, Linux support and maintenance requirements. Do not build an in-house GUI toolkit,
embed a browser as a substitute, add a second scheduler or put policy enforcement only in widgets.

Build a compact operational workspace with navigation for overview, work plan, campaign/team,
changes/reviews and reports. Use actual backend data, responsive native controls, readable status,
keyboard navigation and clear errors. Provide guided setup for existing configuration contracts;
the primary workflow must not require the owner to manage tmux panes or manually assemble command
lines. Raw configuration import/export can remain an advanced option. Never invent supported
models, modes, results or credentials to fill a screen.

The assigned implementation agent should:

1. Begin at Sub-task 36.1.1.1. Reconcile the selected source revision and existing CLI/service
   contracts, including task-source parsing, process ownership, current defects and evidence drift.
   Identify the smallest necessary backend fixes; do not merge unrelated work or change other repos.
2. Implement the native shell and read-only workspace over real repository/task/status data.
   Opening a project starts no campaign and changes no source status or authorization.
3. Add guided local setup, supported provider/model selection and readiness/preflight. Reuse current
   credential handling, keep secrets out of logs/exports, and separate local policy from publication.
4. Connect explicit admission and campaign controls through the same backend authority as the CLI.
   Keep long work off the UI thread. Bind requests to the selected repository/campaign, handle stale
   configuration and repeated actions, and prove detach/reconnect cannot duplicate or abandon work.
5. Add exact changes, review/test results, blockers and reports. Expose new team roles and owner
   modes only after their backend contracts exist. Do not block the initial UI on the other stack
   components, a new provider adapter, optional remote delivery or the full hands-off extension.
6. Verify the real native workflow on disposable targets within existing shared-machine resource
   limits. Cover empty/error/stale/recovery states, minimum-window and high-DPI layouts, keyboard
   and accessibility behavior. Record deterministic, live-provider and installation evidence
   separately; retain independent review and release gates. Report only what actually passed.

The first delivery is a usable standalone CodingMage desktop, not the full ecosystem's general
desktop interface. It does not restart, stop, redirect or adopt existing agent sessions. Any live
provider use or target-repository execution requires the applicable existing explicit authority.

## Milestones

### M-UI-LOCAL

Sprint 36 delivers the native Linux workspace and the supported local coding workflow through
the real backend: open a disposable repository, configure it, inspect readiness and tasks, start
an authorized campaign, observe progress/review/tests, exercise controls and inspect the actual
outcome. Deterministic fixtures must be identified as fixtures. A claim that real providers work
requires a separately admitted real-provider run; mock output and screenshots do not establish it.
Reopen/reconnect, failure recovery, accessibility checks and Linux install/launch evidence are
required for their respective claims. This milestone does not close hands-off or public-release
gates, and can precede M-TEAM-HANDS-OFF.

### M-TEAM-CONTRACT

Sprint 32 contracts and deterministic tests pass with new schemas rejecting unknown fields and
old configurations retaining their previous behavior. Hands-off never means bypass permissions or
auto-approve a dialog. This milestone does not enable a live unattended campaign.

### M-TEAM-LOCAL

Sprints 33 and 34 compose with Sprint 32 in actual coordinator-process fixtures using fake providers
and disposable targets. Director, lead, pods, independent reviewer and QA participate in the same
durable execution state. Real filesystem/Git/process results match the fixture's expected results.
This is executable deterministic evidence, not live-provider qualification.

### M-TEAM-HANDS-OFF

Story 35.1 and Task 35.2.2 pass against exact installed binaries and approved real providers.
The owner authorizes once, disconnects, and provides zero mid-campaign answers. The PRD's declared
ten-outcome workload includes at least five useful integrated implementation outcomes and bounded
repair, review, blocker and restart scenarios. All required product acceptance and independent
review pass. No unrelated checkout or destination branch changes, and no optional service is needed.

Qualify one pod first, then two and subsequently any higher claimed capacity, up to the existing
configured implementation limit. Reserve aggregate CPU, memory, process, test-worker, provider,
storage and token/time capacity; a free pod slot alone is insufficient. A failed stage does not
authorize more pods, higher limits or weaker reviewers. Do not market a one-pod result as five-pod
live qualification.

### M-TEAM-DELIVERY

Optional Task 35.2.1 adds only its proven remote operations and eligible destination policy.
Local hands-off operation does not depend on it. Public package release, signing, deployment and
mandatory independent human review retain their separate requirements.

## Implementation Approach

1. Map requirements CM-TEAM-001 through CM-TEAM-016 to existing modules and exact new task rows.
2. Extend typed contracts and the existing state machine; migrate older state with explicit
   compatibility/refusal behavior rather than silently opting it into delegation.
3. Add denial and mutation fixtures before enabling any new decision or effect path.
4. Integrate one director/lead/pod/reviewer/QA path before increasing concurrency.
5. Run product acceptance through real binaries on disposable repositories with retained inputs.
6. Batch source work and regenerate affected evidence once; never edit stale bindings to conceal
   missing execution or independent authority.
7. Run fresh review on exact candidates, fix findings through the implementer role, and renew every
   affected qualification after the final correction.

## Definition of Done and Reporting

Each story reports requirements covered, actual behavior, tests run/not run, immutable source and
configuration identities, failure dispositions, resource measurements and outstanding evidence.
An implementation task, local qualification, supported live capability, delivered objective and
public release are distinct claims. All new implementation rows start unchecked.

Retrospectives use throughput of accepted functionality, escaped defects, rework, intervention
counts, blocking causes and resource consumption. Replanning may improve task sizes, ordering or
qualified provider choices; it may not reduce acceptance or review strength to make metrics improve.
After external-only blockage, provide a durable report and reconsideration conditions, not a busy
loop or repeated questions to an absent owner.

The current change is documentation only. It neither restarts workers nor expands their active
briefs. Integrate this plan separately from the pinned review of the preceding implementation.
The [planning handoff](docs/evidence/owner-optional-team-planning-handoff.md) records the exact
baseline, validation outcomes, known evidence drift and first implementation unit for Sprints 32
through 35. The native UI brief above and Sprint 36 separately define the next UI assignment;
all its implementation and qualification rows remain open.
