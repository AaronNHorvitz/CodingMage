# Sprint 25 Interruption Matrix

## Boundary

This matrix binds each locally controllable durable effect to executable before-effect,
after-effect, restart, idempotency, and ownership evidence. It does not claim native logout,
shutdown, authenticated remote-service, or hostile operating-system evidence.

| Effect | Exact executable evidence | Reconciled property |
| --- | --- | --- |
| Provider | `codingmage_runtime::team_runtime::tests::interrupted_initial_batch_reuses_exact_runs_reservations_and_lifecycle`; `codingmage-cli::workflow::serial_campaign_resumes_interrupted_correction_without_replaying_implementation` | An admitted or correcting provider unit resumes through its exact run, reservation, worktree, and session without replaying completed implementation. |
| Process | `codingmage_process::runtime::tests::child_cancellation_inherits_downward_without_propagating_upward`; `codingmage-process::runtime::timeout_and_cancellation_reap_descendants` | Cancellation reaches owned descendants and does not propagate into unrelated or parent authority. |
| Gate | `codingmage_orchestrator::tests::crash_after_every_durable_intent_never_replays_state_change`; `codingmage-gate::runner::external_cancellation_reaches_only_the_running_gate_batch` | A durable gate intent is reobserved after interruption, and live cancellation reaches only the exact batch. |
| Commit | `codingmage_git::commit::tests::reobservation_refuses_unowned_or_nondirect_commit`; `codingmage_git::commit::tests::active_checkout_edit_during_commit_survives_exactly` | Commit reobservation accepts no unowned lineage and preserves concurrent active-checkout state. |
| Integration | `codingmage_runtime::team_integration::tests::serialized_integration_completes_from_fresh_and_every_durable_git_boundary` | Fresh execution and restart after every prepared, integrated, and completion-commit boundary converge to one exact integrated head. |
| Publication | `codingmage_runtime::team_publication::tests::publication_reconciles_uncertain_issue_and_never_duplicates_remote_objects`; `codingmage-github::tests::timeout_is_reconciled_by_key_and_never_blindly_replayed` | Uncertain issue, pull-request, and remote timeout outcomes are observed by stable ownership key and never blindly duplicated. |
| Control | `codingmage_runtime::campaign_state::tests::every_control_crash_boundary_replays_to_one_recoverable_effect`; `codingmage_runtime::team_control::tests::control_history_is_idempotent_ordered_and_irreversibly_cancelled` | Every control intent/effect boundary resolves once in canonical order, and cancellation cannot be reversed. |
| Snapshot | `codingmage_runtime::team_state::tests::crash_after_atomic_replace_reconciles_one_observation`; `codingmage_state::snapshot::tests::abandoned_temporary_write_cannot_replace_last_durable_snapshot` | Atomic replacement without its observation gains exactly one observation; abandoned temporary state is inert. |
| Cleanup | `codingmage_runtime::team_integration::tests::failed_integration_verification_releases_temporary_worktrees`; `codingmage_process::runtime::tests::cleanup_does_not_touch_unrelated_matching_executable` | Failure releases only owned temporary worktrees and processes, leaving unrelated matching processes untouched. |

## Verification

The focused crate and integration suites for orchestrator, runtime, Git, GitHub, process, gate,
state, and the serial CLI recovery workflow pass after the latest retained-state corrections. The
full workspace run also passes with only the two separately guarded sustained soaks ignored; both
soaks pass through their explicit opt-in commands.

## Remaining External Faults

Sleep, full logout, host shutdown, authenticated-provider expiration, authenticated-network loss,
and native platform lifecycle faults remain separately open under sub-tasks `25.1.1.3` and
`25.2.3.4`. This matrix does not convert those unavailable effects into local passes.
