# Sprint 30 Context Boundary Evidence (Sub-tasks 30.1.1.1–30.1.1.4)

- **Status:** Provider-independent optional context schemas, deterministic
  fake source, failure matrix, and authority proofs implemented in
  `crates/codingmage-contracts/src/context.rs`; Gate 30.1 local fixtures
  pass. Story 30.2 pinned USTE binding remains blocked on counterpart
  admission.
- **Source:** current `muse/complete-development` worktree before commit
- **Executed:** 2026-09-20 on Fedora Linux x86-64

## Boundary

Local preparation only under the readiness census in
`sprint-29-readiness-census.md`, Decision 0013, and the explicit dependencies
12.2.1.1 and 19.1.2.3 (both checked). Memory stays strictly optional: the
authoritative journal carries recovery, and nothing here grants permissions,
clears blockers, asserts tests or review, or completes tasks. No provider is
bound, no database migrates, and no live integration is authorized.

## Schemas

- `ContextNamespace` and `ContextOperationId` validated identities;
  `ContextCaller` is the coordinator or one exact task.
- `ContextGrant`: separate read/write admission per namespace and caller; a
  directionless grant fails, and `admits` answers per direction.
- `ContextProvenance`: bounded source label plus recording timestamp.
- `ContextContentPolicy`: per-entry byte bound and per-namespace entry bound.
- `ContextLimits`: per-read entry bound, deadline window, and freshness
  window with an exact `is_fresh` predicate.
- `ContextRecord`: namespace, key, content digest (bytes stay provider-side),
  provenance, and operation identity; unknown fields fail.
- Stable codes: `codingmage.context.invalid`, `unavailable`,
  `unsupported`, `unauthorized`, `revoked`, `stale`, `oversize`,
  `quota_exceeded`, `timeout`.

## Commands

```text
cargo fmt -p codingmage-contracts -- --check
cargo clippy -p codingmage-contracts --all-targets -- -D warnings
cargo test -p codingmage-contracts --all-targets -- --test-threads=1
python3 scripts/verification_inventory.py --write
python3 scripts/verification_inventory.py
git diff --check
```

## Results

- `cargo fmt --check`: clean (workspace).
- `cargo clippy --all-targets -- -D warnings`: no warnings (workspace).
- `cargo test -p codingmage-contracts --all-targets -- --test-threads=1`:
  57 passed, 0 failed (42 unit including 15 context fixtures covering
  grant admission, provenance/policy/limits validation, freshness
  bounds, malformed-record refusal, disabled refusal, deterministic
  grants, policy/quota, revoked-vs-unauthorized, outage/timeout/
  cancellation, hostile opacity, idempotent recovery, staleness
  predicate, authority proof, and stable codes; 11 refusal-matrix
  integration; 4 policy pins).
- `python3 scripts/docs_check.py`, `python3 scripts/check_architecture.py`:
  pass.
- Verification inventory regenerated: 1,331 surfaces, 885 explicit gaps;
  the generator check passes (fixture-only change, no new gaps).
- Known pre-existing failure (not introduced here):
  `test_multi_agent_evidence_binding_is_current` still reports
  `input-drift:crates/codingmage-campaign/src/team.rs` and
  `input-drift:README.md` under Sub-task 25.2.4.6; binding hashes left
  unchanged pending external review.

## Fake Source (Sub-task 30.1.1.2)

A deterministic fake source in the `context.rs` test module backs failure
fixtures: disabled mode refuses every call with `Unavailable` so callers
must fall back to the journal, and seeded mode answers fixed entries with
exact grant, policy, quota, and freshness enforcement. Journal-based
recovery and standalone operation are preserved by construction: no
recovery path references context types, and the existing state, runtime,
and workflow suites pass unmodified with memory absent.

Gates: `cargo test -p codingmage-contracts --lib` 36 passed (33 plus 3
fake-source fixtures covering disabled refusal, deterministic grants, and
policy/quota enforcement); crate Clippy clean; inventory holds at 1,330
surfaces with zero new explicit gaps (fixture-only change).

## Failure Matrix (Sub-task 30.1.1.3)

`cargo test -p codingmage-contracts --lib` covers each required outcome
against the deterministic fake source: wrong namespace and wrong caller
refuse with `Unauthorized` (`seeded_source_answers_deterministically_
within_grants`); revoked access refuses with `Revoked`, distinguished
from never-granted `Unauthorized` (`revoked_access_is_distinguished_from_
never_granted`); malformed records fail closed (`rejects_malformed_
records`); oversized writes fail with `Oversize` and entry overflow with
`QuotaExceeded` (`fake_source_enforces_policy_and_quota`); outage refuses
with `Unavailable` without mutation, over-deadline writes fail with
`Timeout`, one-shot cancellation fails with `Cancelled` then resumes
normal reads (`outage_timeout_and_cancellation_apply_without_mutation`);
hostile bytes round-trip as opaque content and grant nothing
(`hostile_content_stays_opaque_and_grants_nothing`); idempotent retry
with the same operation ID returns the single stored write while a
content change fails with `Invalid` (`idempotent_write_recovery_never_
duplicates`); staleness is decided by the exact `is_fresh` predicate
(`stale_records_are_detected_by_the_freshness_predicate`,
`freshness_boundaries_hold`).

## Authority Proof (Sub-task 30.1.1.4)

`stored_content_confers_no_authority_and_absence_stays_blocking` writes
hostile bytes claiming grants, passing tests, and completion, then proves
an unprivileged task caller still receives `Unauthorized` on read and
write, revocation still enforces `Revoked`, and a disabled source reports
`Unavailable` instead of fabricating content. Memory never mints grants,
clears blockers, asserts tests/review, or completes tasks; explicitly
memory-dependent reads stay blocked on the missing capability with the
journal as the canonical fallback.

## Open Items

Story 30.2 pinned USTE binding needs counterpart admission, a pinned
committed interface, and exact-artifact qualification access on both
sides; none is available locally, so no provider is bound here.
