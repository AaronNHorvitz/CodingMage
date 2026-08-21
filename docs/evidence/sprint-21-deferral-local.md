# Sprint 21 Local Deferral Evidence

- **Status:** Reason and trigger contract complete; external observations and full permutation gate remain open
- **Implementation commit:** `c412a634e61e2e300b71593c54e41b5bf9a372fd`
- **Executed:** 2026-08-21 on Fedora Linux with Rust 1.95.0

## Contract

Every accepted deferral uses one closed reason and exactly one permitted trigger:

| Reason | Required trigger |
| --- | --- |
| Temporary provider capacity | Provider reset |
| Active path lease | Lease release |
| Gate-resource contention | Gate-resource release |
| Deterministic dependency order | Campaign-head advancement |
| Pending stronger review | Review completion |
| Operator pause | Operator resume |

The deterministic validator rejects every mismatched pairing before a lease or implementation
provider starts. The runtime persists the task, reason, trigger, source head, and task-source digest
inside the integrity-protected checkpoint. Deferred tasks remain unchecked and are excluded from
ready selection without making their descendants ready.

The serial runtime directly observes only facts it owns. Campaign-head advancement requires a
different reconciled head. Path-lease and gate-resource release are satisfied only at the serial
safe boundary where no pod or gate resource remains active. Provider reset, review completion, and
operator resume remain unobserved until authenticated external-control integration is implemented;
the affected task stays deferred and the campaign pauses instead of guessing.

An already-satisfied same-head deferral is retained as a repeat guard. Repeating the identical task,
reason, trigger, head, and source digest stops with
`codingmage.campaign.lead_repeated_satisfied_deferral` rather than spinning.

## Verification

The campaign-level test mutates every wrong trigger for gate-resource contention. Runtime tests
prove exact-head nonobservation, later-head observation, deterministic order, independent-task
eligibility, lease-release observation, and satisfied-repeat detection. The binary vertical slice
defers the first of two independent tasks until head advancement, runs the second task, interrupts
at durable integration intent, reconciles after restart, observes the new head, and then completes
the deferred first task. The active checkout remains unchanged throughout.

```text
cargo test -p codingmage-campaign --all-targets --locked
cargo test -p codingmage-runtime --all-targets --locked
cargo test -p codingmage-cli --test workflow --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo fmt --all -- --check
git diff --check
```

All commands passed.

## Open Scope

This evidence closes sub-tasks `21.2.3.1` and `21.2.3.2` only. Sub-task `21.2.3.3` remains open
until every external trigger has an authenticated positive-observation path. Sub-task `21.2.3.4`
remains open until repeated satisfied deferral creates a durable typed human-decision projection,
not only a content-free stop code. Sub-task `21.2.3.5`, acceptance criterion `AC 21.4`, Task
`21.2.3`, Story `21.2`, and Sprint 21 gates remain open pending the complete permutation suite.
