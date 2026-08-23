# Sprint 25 Platform Fault Evidence

- **Status:** Dependency-ready Fedora fault cases pass; sleep, logout, and shutdown remain open
- **Source commit:** `a290c8c`
- **Executed:** 2026-08-23 on Fedora Linux

## Passing Local Cases

The serial Linux process suite passed 11 tests with no skips. It proves:

- arguments are literal and response-file syntax is rejected before spawn;
- executable replacement and missing deadlines fail before spawn;
- output overflow and process-count overflow terminate the exact owned process group;
- timeout and cancellation reap children and grandchildren;
- independently killing the coordinator parent causes the guard to reap the owned descendant;
- child processes inherit neither ambient environment nor coordinator control files; and
- an unrelated process using the identical executable survives owned cleanup.

The campaign-state tests passed exact output, process-invocation, retained-storage, provider,
correction, malformed-report, and elapsed-execution limit exhaustion in stable order without
broadening policy authority. Retained-state scanning also passed disappearing-descendant,
directory-replacement, and non-followed-symlink races. The binary-level cancellation workflow
terminated its owned provider while preserving a concurrently running unrelated process, and the
aggregate correction-limit workflow paused cleanly at its exact ceiling.

## Commands

```text
cargo test -p codingmage-process --test runtime --locked --offline -- --test-threads=1
cargo test -p codingmage-runtime --locked --offline retained_state_scan -- --nocapture
cargo test -p codingmage-runtime --locked --offline \
  campaign_state::tests::checkpoint_reports_each_exhausted_limit_in_stable_order -- --exact
cargo test -p codingmage-runtime --locked --offline \
  campaign_state::tests::every_exhausted_campaign_limit_preserves_exact_policy_authority -- --exact
cargo test -p codingmage-campaign --locked --offline campaign_limit_boundaries_fail_closed
cargo test -p codingmage-cli --test workflow --locked --offline \
  campaign_cancel_terminates_owned_provider_without_signalling_unrelated_process -- --exact
cargo test -p codingmage-cli --test workflow --locked --offline \
  serial_campaign_pauses_cleanly_when_aggregate_correction_limit_is_reached -- --exact
```

## Open Native Evidence

Sub-task `25.2.3.4` remains unchecked. A separate unprivileged login environment must exercise
service and campaign behavior across host sleep, full user logout/login, and controlled host
shutdown/restart. Unit rendering, synthetic restart tests, and an ordinary service stop/start are
not substitutes for those native lifecycle transitions. The campaign must then reconcile exact
process, journal, checkpoint, worktree, lock, and target-checkout state with no unrelated-process
effect before this sub-task or `AC 25.1` can close.
