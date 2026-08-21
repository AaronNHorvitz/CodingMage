# Sprint 21 Local Deferral Evidence

- **Status:** Complete
- **Implementation commits:** `c412a634e61e2e300b71593c54e41b5bf9a372fd`, `ea9bf6c87cf51e7eb4663dcf27b504b0cf27b343`
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
operator resume require `campaign-observe-trigger`, a same-user command that binds an exact campaign,
task, trigger, request identity, and evidence digest. It acquires the campaign lock, revalidates the
clean active checkout and isolated campaign worktree, records a create-once integrity-bound intent,
and returns only that exact task to deterministic ready-set evaluation. An identical replay is
idempotent; conflicting request reuse or a different trigger fails closed.

An already-satisfied same-head deferral is retained as a repeat guard. Repeating the identical task,
reason, trigger, head, and source digest creates a durable typed human-decision projection under
`codingmage.campaign.human_decision.repeated_satisfied_deferral`. The exact task remains unavailable,
independent work continues, and a campaign with no independent work stops without spinning.

## Verification

The campaign-level test mutates every wrong trigger for gate-resource contention. Runtime tests
prove exact-head nonobservation, later-head observation, deterministic order, independent-task
eligibility, lease-release observation, durable human-decision suppression, and satisfied-repeat
detection. An exhaustive `6^4` matrix checks 1,296 combinations of completed, blocked, deferred,
trigger-satisfied, human-decision, and available states and always selects the first eligible task
or truthfully reports no ready work. The binary vertical slice defers the first of two independent
tasks for operator pause, runs the second task, interrupts at durable integration intent, refuses a
premature observation, reconciles after restart, pauses without guessing, accepts one authenticated
operator-resume observation, proves exact replay idempotency and conflicting-reuse refusal, and then
completes the deferred task. The active checkout remains unchanged throughout.

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

This evidence closes Task `21.2.3` and acceptance criterion `AC 21.4`. Story `21.2` and the Sprint
21 gates remain open for Task `21.2.4`, the broader human-decision lifecycle, and the complete
hostile-output corpus. The trigger evidence digest proves operator attestation identity and
idempotency; CodingMage does not inspect or assert the truth of external evidence content.
