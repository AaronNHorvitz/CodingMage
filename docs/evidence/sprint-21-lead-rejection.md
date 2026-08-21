# Sprint 21 Lead-Rejection Evidence

- **Status:** Task 21.2.4 complete
- **Implementation commit:** `42c237efb1ad7347332c4a157e37c382b3c42863`
- **Executed:** 2026-08-21 on Fedora Linux with Rust 1.95.0

## Contract

Invalid lead output is not an accepted disposition. The coordinator rejects malformed structured
output and parsed-but-unauthorized proposals under separate content-free codes. Each refusal appends
an integrity-protected projection containing only a sequence, closed reason, exact campaign head,
and task-source digest. It pauses at the campaign safe boundary without copying provider rationale,
paths, source, prompt text, or arbitrary output into durable state.

The hostile validator corpus covers mixed dispositions, unknown fields and reasons, duplicate task
identities, stale snapshots, contradictory dependencies, path escape, denied and unapproved roots,
artifacts outside leased roots, unknown gate tiers, and attempted provider, publication, command, or
credential fields. Serde unknown-field denial and deterministic proposal sealing refuse those shapes
before scheduler admission.

## No-Effect Verification

The binary campaign fixture runs three consecutive rejected lead turns against a one-unit ceiling:

1. A parsed proposal requests an escaping ownership root.
2. A report adds an undeclared authority field.
3. A report replaces the exact campaign head with a stale identity.

Every run returns a typed pause with zero completed units. The repeated turns prove rejection does
not consume the accepted-unit ceiling. Exact pre/post assertions prove:

- the implementation executable is never invoked;
- no pod branch or pod worktree exists;
- the active checkout head, task bytes, and source bytes remain identical;
- no task checkbox changes and no candidate or completion commit appears;
- the only retained worktree is the pre-admission campaign snapshot;
- provider rationale and the undeclared field value do not appear in checkpoint bytes; and
- the digest-verified rejection array grows by exactly one record per refusal.

## Verification

```text
cargo test -p codingmage-campaign hostile_lead_authority_expansion_corpus_fails_before_admission --locked
cargo test -p codingmage-cli --test workflow rejected_lead_output_has_no_downstream_effect_and_consumes_no_unit --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo doc --workspace --no-deps --locked
cargo fmt --all -- --check
git diff --check
```

All commands passed. This evidence closes Task `21.2.4`, `AC 21.2`, and `AC 21.6`. The later
human-decision and complete disposition/reason matrices are reconciled in
`docs/evidence/sprint-21-gate.md`.
