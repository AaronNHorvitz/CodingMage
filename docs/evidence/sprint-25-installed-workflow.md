# Sprint 25 Installed-Artifact Workflow Evidence

- **Status:** Passed on Fedora Linux; no external provider or GitHub authority used
- **Package source commit:** `07304426c457f0a6c4a820784f4e6f00c20824e9`
- **Qualification harness commit:** `107c0c9ddaa9007d182cf17dde82d63be80600b9`
- **Package SHA-256:** `010de791152c61fa7956cbf999c0292abac542307e1de980a4f3dfd452f1c58e`
- **Executed:** 2026-08-23

## Package Boundary

The release archive was produced from a clean source checkout. Its closed build manifest binds
version `0.1.0`, source commit, source epoch, Cargo lock digest, and binary digest. It declares no
credentials or runtime state and Linux-only native evidence. The hardened installer verified every
declared archive member before installing the executable into a fresh unprivileged prefix.

The test harness accepts `CODINGMAGE_TEST_BINARY` only as an explicit test-process override. Without
that variable, every ordinary integration test continues to invoke Cargo's exact test binary. The
override changes no product command, authority, provider, repository, or release behavior.

## Installed Execution

With `CODINGMAGE_TEST_BINARY` set to the installed executable, these exact binary-level workflows
passed:

```text
cargo test -p codingmage-cli --test workflow --locked --offline \
  supervised_run_composes_fake_agents_git_gates_state_and_cleanup -- --exact

cargo test -p codingmage-cli --test workflow --locked --offline \
  serial_campaign_advances_two_reviewed_tasks_without_touching_active_checkout -- --exact
```

The supervised workflow exercised initialization, diagnosis, planning, isolated implementation,
deterministic gate correction, immutable senior review, reviewed correction, final verification,
task reconciliation, cleanup, and content-minimized terminal output. The serial workflow exercised
campaign execution, status inspection, idempotent pause and resume, stop-after-unit, deferred-trigger
observation, durable restart, final report inspection, and exact active-checkout preservation.
All providers and target repositories were synthetic local fixtures.

## Native Service And Package Lifecycle

The same installed prefix and package digest then passed this real `systemd --user` lifecycle:

1. Install and verify the service bound to exact synthetic configuration and campaign files.
2. Pass `systemd-analyze --user verify` on the generated unit.
3. Start the inert service and observe `activating`; stop it and observe `inactive`.
4. Install the same verified archive as an upgrade and verify the binary and service receipts.
5. Roll back atomically and verify both receipts again.
6. Remove the service, reload the user manager, and remove the installed binary.
7. Verify the unit, binary, and installation receipt are absent.
8. Verify an unrelated prefix file plus the configuration and state roots remain present.

The intentionally incomplete synthetic service campaign exercised service supervision without
contacting a provider or modifying a valuable repository. Complete campaign semantics were tested
separately through the same installed binary immediately before the service lifecycle.

## Limitations

This evidence closes only sub-task `25.2.2.4`. It is Fedora Linux evidence, not macOS or Windows
evidence. It does not claim authenticated provider, GitHub, logout, shutdown, signing, independent
human review, manual fuzzing, or publication coverage. Package purge was not invoked because data
retention, rather than destructive deletion, is the required default-policy assertion.
