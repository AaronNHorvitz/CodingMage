# Sprint 11 Evidence

- **Status:** Passed locally
- **Source commits:** `23fe3dc006bbb6ac78beeb76fa21ab1b5db669fb` and
  `f1c0f9136ddcc37184b308bcc966618edfe80386`
- **Executed:** 2026-08-19 on Fedora Linux with Rust 1.95.0
- **Scope:** Finding lifecycle, correction packets, bounded rounds, and independent review

## Commands

The focused five-test review suite, formatting, strict Clippy, architecture policy, and
`git diff --check` passed. The preceding full workspace gate contained 88 tests; these changes add
five review tests for a current total of 93.

## Verified Behavior

- Findings are deduplicated by exact reviewed commit and stable ID and move only through legal
  open, accepted, corrected, verified, disputed, withdrawn, and blocked transitions.
- Corrected findings bind an exact nonidentical correction commit. Verification requires a relevant
  code or test change or a stable validated no-change explanation.
- Correction packets contain only accepted defects, exact reviewed and correction-base commits,
  sorted finding IDs, literal requested tests, and the unchanged canonical scope hash. External
  blockers and optional suggestions cannot silently become mandatory correction scope.
- The default and maximum autonomous budget is three failed rounds. The second failure requires
  model escalation, the third records dispute, and a fourth call fails closed.
- Material architecture disputes require explicit human resolution.
- An author cannot be the sole final reviewer. A Codex emergency correction requires an independent
  Claude or human reviewer before closure.

## Limitations

- The ledger is currently an in-memory contract. Sprint 12 must persist its transitions in the
  append-only journal before interruption-safe use.
- Provider findings remain untrusted until the Codex scope checks and deterministic evidence gates
  are complete.
