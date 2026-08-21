# Sprint 17 Disposable Soak Evidence

- **Status:** Exact schedule materialization passes; fake-adapter execution and complete soak
  reconciliation remain open
- **Schedule implementation commit:** `8f79bfa`
- **Executed:** 2026-08-21 on Fedora Linux with Rust 1.95.0

## Exact Schedule

`codingmage-soak` now exposes one immutable prescribed schedule with one pod, `local_only`
publication, an exact accepted-outcome ceiling of ten, stable outcome IDs, canonical fixture task
IDs, durable completed/blocked/deferred classifications, and ordered fault or control identities.
The schedule is independent of any target repository and contains no target source.

The ten accepted outcomes are a clean pass, gate correction, review correction, external blocker,
temporary deferral, malformed-report repair, provider-capacity pause and resume, interrupted
recovery, stop-after-unit completion, and the deferred task's triggered completion followed by exact
ceiling enforcement. The deferred task is intentionally the only repeated task identity: its first
accepted outcome is the typed deferral and its second is the final reviewed completion after the
recorded trigger. This preserves an exact ten-outcome ceiling while exercising genuine deferral
reconsideration.

Focused tests require sequences one through ten, ten unique outcome identities, eight completed
outcomes, one blocker, one deferral, the exact repeated task, the trigger-plus-ceiling terminal
boundary, and deterministic equality across repeated materialization. Independent mutations of pod
count, publication mode, ceiling, schedule length, ordering, deferred-task identity, and terminal
trigger identity are rejected as invalid configuration.

## Verification

The following focused commands pass:

```text
cargo test -p codingmage-soak --all-targets --locked
cargo clippy -p codingmage-soak --all-targets --locked -- -D warnings
cargo fmt --all -- --check
git diff --check
```

This closes only `17.1.2.1`. It does not claim fake-adapter execution, repeated soak stability,
production coordinator qualification, controlled-target execution, or installed-package evidence.
