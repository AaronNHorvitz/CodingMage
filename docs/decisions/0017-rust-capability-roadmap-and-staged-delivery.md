# 0017: Rust Capability Roadmap and Staged Delivery

| Field | Value |
| --- | --- |
| Status | Accepted for product direction and implementation sequencing |
| Date | 2026-09-24 |
| Authority | Repository owner's explicit development replan and restart instruction |
| License effect | None; proposed changes require the separate owner license decision |

## Decision

Adopt [the 48-capability catalogue](../../CAPABILITY-ROADMAP.md) and
[the component implementation amendment](../../IMPLEMENTATION-AMENDMENT.md).
They extend the preserved PRD and task plan; they do not claim implementation or qualify a release.

Reconcile the existing native UI and evidence-freshness gaps first, then implement Sprints 32-35 in their dependency order and the owned capability crosswalk below. Preserve Sprint 36 human/live acceptance; unavailable credentials or desktop access do not block independent contract work.

Own durable campaigns, task/role scheduling, isolated repository operations, immutable review and serialized integration. Preserve the native Rust client already implemented. Consume the standalone execution engine through an optional versioned adapter; do not implement a second tool/research/model loop.

Older statements restricting the worker to the previous narrow slice or leaving it indefinitely
paused are superseded by this explicit restart assignment. Existing safety, independent review,
human-only acceptance, external publication and resource boundaries remain binding. Work on
later capabilities only after dependencies, with P0 before P1 before P2 and no silent deletion
of the original roadmap. Keep one actual execution/state owner for each domain.

## Consequences

Add stable work-package crosswalks without renumbering prior tasks. Reconcile and reuse existing
code before extending it. Keep public documentation consumer-neutral. Preserve current licenses
until a separate authorized transition; no private consumer publication is authorized here.
Serialize hardware qualification and use real pinned-model evidence; planning or fake adapters
never prove that combined functionality works.
