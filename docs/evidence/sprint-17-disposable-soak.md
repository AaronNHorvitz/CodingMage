# Sprint 17 Disposable Soak Evidence

- **Status:** Prescribed fake-adapter disposable campaign passes; production coordinator and
  controlled-target qualifications remain open
- **Schedule implementation commit:** `8f79bfa`
- **Fake-adapter execution commit:** `1eb59a3`
- **Behavioral injection commit:** `1c6f689`
- **Outcome and residue reconciliation commit:** `dff3395`
- **Repeated-run isolation commit:** `ee1d1bb`
- **Complete invariant-gate commit:** `7e32c40`
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

## Behavioral Injections

Every named scenario now emits a second closed, contiguous event stream at its exact fake-adapter
boundary. Gate and review cases record fail/finding followed by corrected pass; blockers and
deferrals are classified at the lead; malformed output is rejected before bounded repair; provider
capacity pauses at the process boundary and resumes only after service revalidation; interruption
occurs after durable intent and restarts through no-replay recovery; stop-after-unit is accepted at
the monitor and ends at a service checkpoint; and the final task records its exact deferral trigger
before the service refuses further admission at the ceiling.

The ten scenario identities produce seventeen ordered behavioral observations. Tests require all
ten scenario families and exact final trigger/ceiling ordering. Removing, reordering, duplicating,
changing the adapter boundary, or replacing a behavioral phase causes reconciliation failure.

## Outcome And Residue Reconciliation

Each accepted fake outcome now carries deterministic evidence for exact outcome and task identity,
candidate commit identity where completion is claimed, required gate and review observation counts,
checkpoint SHA-256, guarded process start/reap counts, and worktree creation/removal. The gate and
review correction scenarios require two observations at the corrected boundary; blocked and
deferred outcomes carry no candidate, gate, review, or worktree claim.

Campaign residue requires identical content-free active-checkout manifests, eighteen started and
reaped fake provider processes, eight created and removed task worktrees, zero orphan processes, and
zero leaked worktrees. Tests independently falsify candidate identity, gate count, review count,
blocked worktree authority, checkpoint digest, active-checkout preservation, process reaping,
worktree cleanup, orphan count, and leak count. Every false claim fails reconciliation.

## Repeated-Run Isolation

Fake candidate and checkpoint identities now bind a canonical campaign run ID. Thirty-three complete
runs require identical accepted classifications, adapter ordering, injection ordering, pod and
publication policy, and residue accounting. Completed candidate and all checkpoint identities must
remain distinct across runs; blocked and deferred outcomes consistently retain no candidate.

Each later run is then given one outcome-evidence record from the baseline run. Exact-run
reconciliation rejects the substitution. Empty, overlong, prefixed-dash, slash-bearing,
whitespace-bearing, and post-execution rebound run identities are also rejected.

## Complete Invariant Gate

The accelerated qualification report now tracks duplicate tasks, skipped gates, false completions,
unreviewed commits, orphan processes, leaked worktrees, unowned mutations, silent model downgrades,
and retained-state observations above the configured bound as nine independent defect classes.
Each counter is mutated separately and every nonzero value fails the campaign gate.

The complete focused suite passes after the final fake-soak reliability change. This closes Task
`17.1.2`, acceptance criterion `17.1`, and Gate `17.1`. It does not claim production coordinator
qualification, controlled-target execution, installed-package evidence, or any target-repository
result.
