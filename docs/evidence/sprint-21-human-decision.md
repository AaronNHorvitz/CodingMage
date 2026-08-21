# Sprint 21 Human-Decision Continuation Evidence

- **Status:** Complete
- **Implementation commit:** `3d4d5b37cdd65ae5ab6a9d411203f9e0536a4499`
- **Executed:** 2026-08-21 on Fedora Linux with Rust 1.95.0

## Contract

A valid `human_decision_required` disposition binds one exact dependency-ready task, closed reason,
campaign head, task-source digest, and canonical dependencies. The runtime records only that typed
projection. It does not retain the provider-authored question, start implementation for the held
task, check its box, or make its dependency descendants ready. The scheduler excludes only that
task and continues independent dependency-ready work.

## Vertical Slice

The binary fixture presents two independent tasks. The read-only lead places the first task under a
material-architecture decision and proposes the second on its next turn. The second task completes
through the production Claude adapter, deterministic gate path, Codex review, coordinator commit,
campaign integration, and exact checkbox reconciliation. The campaign then stops with
`codingmage.campaign.no_independent_ready_work_pending_human_decision`.

Assertions prove the held task remains open while the independent task is checked; exactly one
implementation call occurs; the campaign branch contains only the independent source change; and
the active checkout head, task source, and source file remain unchanged. The provider question is
absent from checkpoint bytes. A restart returns the same typed stop without invoking the lead or
implementer again, proving the human-decision projection survives and no accepted effect replays.

## Verification

```text
cargo test -p codingmage-cli --test workflow human_decision_survives_restart_while_independent_work_continues --locked
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
git diff --check
```

All commands passed. This evidence closes `AC 21.5`. Every Story `21.2` task and acceptance
criterion is complete; the story remains represented by its constituent checked items because its
heading has no checkbox. The complete matrices are reconciled in
`docs/evidence/sprint-21-gate.md`.
