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

## 2026-09-20 Development-Host Timing Observation

On this development host, `timeout_and_cancellation_reap_descendants` and
`cancellation_reaps_children_and_grandchildren` in
`crates/codingmage-process/tests/runtime.rs` fail deterministically, including
in isolation on an idle machine. Both assert outcomes that pass
(`TimedOut`/`Cancelled` with verified cleanup) and then wait up to 3 seconds
for a driver pidfile that never appears. A temporary timing probe (removed
after use, never committed) measured guarded startup here at roughly 430
milliseconds to pidfile appearance, with `ProcessExecutor::new` alone at
roughly 127 milliseconds, while the tests kill or time out at 100 to 150
milliseconds; a directly spawned driver writes its pidfile in about 6
milliseconds. The product behavior under test is truthful (it stops and reaps
exactly); only the test's sub-150-millisecond startup assumption does not hold
on this host. No test or implementation line was changed for this observation,
no platform or unrelated code path is involved, and this does not qualify or
disqualify any other environment. A host whose guarded startup fits inside the
test timers is the exact prerequisite for re-running these two suites.

A sibling environment prerequisite affects
`tests::native_systemd_analyze_accepts_rendered_user_unit` in
`crates/codingmage-service/src/lib.rs`: it shells out to
`systemd-analyze --user verify`, which fails here because no systemd user
manager exists in this session (`/run/user/1000/systemd` is absent and the
user bus is unreachable), even though the `systemd-analyze` binary itself is
present. The rendered unit is byte-identical work; only the native user-manager
verification step cannot run. A genuine login session with a running user
manager is the exact prerequisite for that suite, consistent with the open
native-lifecycle evidence above.
