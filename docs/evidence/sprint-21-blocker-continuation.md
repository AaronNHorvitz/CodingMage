# Sprint 21 Blocker Continuation Evidence

- **Status:** Local blocker validation, persistence, continuation, and authenticated clearance complete
- **Implementation commits:** `16071fb902941474d50359271bed6e64f3d571a9`, `377a51df28e9ecf2120154872f26697e73c1cdfe`
- **Executed:** 2026-08-21 on Fedora Linux with Rust 1.95.0

## Contract

The coordinator accepts a lead blocker only when its immutable binding names a task in the exact
dependency-ready set. The closed reason taxonomy rejects `blocked_prerequisite` for such a task
because that claim contradicts dependency readiness. Unknown reasons, mixed dispositions, stale
snapshot fields, changed dependencies, and repeated blockers for the same task fail closed.

Each accepted blocker stores the exact task identity and typed reason in the integrity-protected
campaign checkpoint. The canonical task checkbox remains open. Subsequent planning excludes that
task, while normal dependency evaluation keeps its descendants unavailable and leaves independent
ready tasks eligible. When the independent paths are exhausted, the campaign stops with
`codingmage.campaign.no_unblocked_ready_work`.

Checkpoint loading preserves both the original pre-blocker shape and the intermediate blocked-ID
shape after verifying their original canonical digests. A new checkpoint round-trip proves the
typed reason is integrity-bound, and byte mutation still causes state refusal.

## Vertical Slice

The binary campaign fixture contains three tasks: an externally blocked task, an independent local
task, and a descendant of the blocked task. The first lead call records the blocker; the second
selects the independent task, which passes implementation, gates, review, integration, and exact
checkbox completion. The final campaign branch leaves the blocker and descendant unchecked, reports
one durable blocker, and stops only after no independent work remains. The active checkout stays at
its original head and retains its original task and source bytes.

Before clearance, restarting the same campaign returns the same durable terminal state without
another lead or implementer call, proving the accepted blocker and completed unit are not replayed.

## Authenticated Clearance

The local `campaign-clear-blocker` command authenticates through the invoking Linux user and refuses
campaign state or checkpoint files that are linked, owned by another user, or accessible to group or
world. It acquires the exact repository campaign lock and revalidates the clean active checkout,
repository identity, original authority, isolated worktree identity, campaign head, task-source
digest, open task, and completed dependencies before changing state.

The operator supplies one validated task ID, one validated idempotency ID, and a lowercase SHA-256
digest binding the changed external prerequisite without storing its content. CodingMage writes a
private, create-once, integrity-bound clearance intent before removing only the matching typed
blocker. Repeating identical input returns `changed: false`; changing the task or digest under the
same request ID fails closed. The command starts no provider and changes no checkbox.

The vertical slice makes its external prerequisite observable, clears the blocker, and resumes the
campaign. The formerly blocked task completes first, its descendant then becomes ready and
completes, all three canonical campaign-branch checkboxes are checked, and the active checkout still
has its original head and task bytes. Clearance-intent mutation, widened state permissions, and
conflicting idempotency reuse are independently rejected.

## Verification

```text
cargo test -p codingmage-campaign -p codingmage-runtime --all-targets --locked
cargo test -p codingmage-cli --test workflow --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo fmt --all -- --check
git diff --check
```

All commands passed. The full workspace run includes the focused blocker continuation fixture and
all existing process, Git, gate, state, review, provider, orchestration, and campaign tests.

## Open Scope

This evidence closes Task `21.2.2` and acceptance criterion `AC 21.3`. Deferral and
reconsideration semantics, human-decision continuation, rejected-output effect proofs, Story
`21.2`, and the Sprint 21 gates remain open.
