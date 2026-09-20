# Sprint 30 Context Boundary Evidence (Sub-task 30.1.1.1)

- **Status:** Provider-independent optional context schemas defined in
  `crates/codingmage-contracts/src/context.rs`; fake source, failure
  fixtures, and authority proofs remain open under Sub-tasks 30.1.1.2–30.1.1.4
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

- `cargo fmt --check`: clean.
- `cargo clippy --all-targets -- -D warnings`: no warnings (one
  single-character pattern corrected during development).
- `cargo test --all-targets`: 48 passed, 0 failed (33 unit including 6 new
  context fixtures, 11 refusal-matrix integration, 4 policy pins).
- Verification inventory regenerated: 1,299 surfaces became 1,330 with zero
  new explicit gaps; the generator check passes.

## Open Items

Sub-tasks 30.1.1.2 (disabled/default and deterministic fake source),
30.1.1.3 (wrong namespace, revoked access, stale/malformed/oversized,
hostile, quota, outage, timeout, cancellation, idempotent recovery), and
30.1.1.4 (memory grants nothing) remain open in dependency order. Actual
USTE binding in Story 30.2 needs pinned interfaces and admission on both
sides.
