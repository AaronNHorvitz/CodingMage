# Sprint 17 Disposable Soak Evidence

- **Status:** Exact schedule materialization passes; fake-adapter execution and complete soak
  reconciliation remain open
- **Schedule implementation commit:** `8f79bfa`
- **Fake-adapter execution commit:** `1eb59a3`
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

## Fake-Adapter Execution

The source-independent schedule now executes through typed fake lead, implementer, reviewer, gate,
Git, process, monitor, and service boundaries. Every accepted outcome begins with a durable service
load, crosses a lead selection and guarded process observation, emits privacy-safe monitor status,
and ends with a service checkpoint. Completed outcomes alone may cross implementation, gate,
review, and Git integration. Blocked and deferred outcomes are explicitly prohibited from those
mutation-capable boundaries.

The execution report requires a contiguous observation stream, all eight adapter families, exactly
ten accepted outcomes, a peak of one active pod, and `local_only` publication. Mutation tests remove,
duplicate, and reorder observations; raise the pod count; broaden publication; and inject Git
integration into the blocked outcome. Every mutation fails reconciliation.

This closes `17.1.2.1` and `17.1.2.2`. It does not yet claim behavioral fault injection, complete
artifact reconciliation, repeated soak stability, production coordinator qualification,
controlled-target execution, or installed-package evidence.
