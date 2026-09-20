# Sprint 31 Muse Protocol Evidence (Sub-task 31.1.1.1)

- **Status:** Required Muse worker protocol behavior and deterministic
  fake transcripts defined in `crates/codingmage-muse/src/lib.rs` on the
  provider-neutral `codingmage-agent` contract. Exact CLI inspection
  (31.1.1.2), the typed adapter (31.1.1.3), confinement proofs
  (31.1.1.4), fault fixtures (31.1.1.5), and live qualification (Story
  31.2) remain open in dependency order.
- **Source:** current `muse/complete-development` worktree before commit
- **Executed:** 2026-09-20 on Fedora Linux x86-64

## Boundary

Local specification only under Decision 0013 and dependencies 4.2.1.1
and 4.2.2.3 (both checked). No CLI is invoked, no authentication
material is read, and no live integration is authorized. Muse
development use stays outside the running product; a missing required
behavior fails admission as a blocker, never as a bypass.

## Required Behavior

- Adapter name `muse` reported by the capability probe.
- Non-empty version or fingerprint so later version-drift refusal has
  something exact to pin against.
- All five structured behaviors: `StructuredOutput`, `EventStream`,
  `SessionContinuation`, `Cancellation`, `UsageObservation`.
- `admit` enforces the three checks with stable codes
  `codingmage.muse.wrong_provider`, `codingmage.muse.missing_version`,
  and `codingmage.muse.missing_capability.<Capability>`.

## Fake Transcripts

Deterministic `FakeAdapter` scripts exercise the full worker round
trip against the real normalization rules: probe admits the exact
required set; start opens one exact session; continue on the retained
session succeeds while a foreign session refuses with `InvalidRequest`;
usage observation reports the latest transcript counters; cancel closes
the session and a second cancel refuses with `InvalidRequest`.

## Commands

```text
cargo fmt --check
cargo clippy -p codingmage-muse --all-targets -- -D warnings
cargo test -p codingmage-muse --all-targets -- --test-threads=1
python3 scripts/docs_check.py
python3 scripts/check_architecture.py
python3 scripts/verification_inventory.py --write
python3 scripts/verification_inventory.py
git diff --check
```

## Results

- `cargo fmt --check`: clean (workspace).
- `cargo clippy -p codingmage-muse --all-targets -- -D warnings`: no warnings.
- `cargo test -p codingmage-muse --all-targets -- --test-threads=1`:
  6 passed, 0 failed (exact admission, per-behavior refusal,
  provider/version refusal, full lifecycle round trip, wrong-session
  refusal, stable codes).
- `python3 scripts/docs_check.py`, `python3 scripts/check_architecture.py`:
  pass.
- Verification inventory regenerated: 1,331 surfaces became 1,337 with
  4 new explicit gaps (uncovered malformed-input/unknown-field
  categories on the new spec const and error enum — the same
  informational kind sibling provider crates already carry); the
  generator check passes.
- Known pre-existing failure (not introduced here):
  `test_multi_agent_evidence_binding_is_current` still reports
  `input-drift:crates/codingmage-campaign/src/team.rs` and
  `input-drift:README.md` under Sub-task 25.2.4.6; binding hashes left
  unchanged pending external review.

## Open Items

Sub-task 31.1.1.2 needs the exact installed CLI identity and supported
structured protocol before any adapter code is written; terminal
scraping and assumed flags are out of scope.
