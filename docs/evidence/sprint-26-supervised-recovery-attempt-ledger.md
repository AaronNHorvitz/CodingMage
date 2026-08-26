# Sprint 26 Supervised Recovery and Provider-Attempt Evidence

**Date:** 2026-08-26

**Implementation commits:** `6180d63788bead848463acc9c41f5c61dbda6103`,
`82383835d5f34dddac81e07b4394a7c5f91f62e4`

**Scope:** Sub-tasks `26.1.7.6` and `26.1.7.7`

## Claim

An operator can resume an integrity-verified supervised run by supplying its exact validated
`run_id`. The explicit identity selects only retained state already bound to the same repository,
source commit, task, branch, worktree, correction round, candidate, and provider session. Omitting
`--run-id` retains the distinct fresh-run default. Invalid identities and identity mismatches fail
before provider or repository effects.

Provider attempts are reserved before process spawn in a private, integrity-bound ledger. Initial
implementation, each exact correction round and parent session, and each immutable review candidate
have separate three-attempt ceilings. The ledger survives independent CLI invocations. A fourth
attempt in the same scope fails with `codingmage.provider.attempt_limit` before another provider
process can start, while a different exact scope retains its own allowance.

## Live Recovery Observation

The retained supervised run `run-dcf8ad5d265169bd6fc253d40a7a7d57` demonstrated explicit recovery
at the prepared correction stage. Repeated invocations preserved its run, task, worktree, correction,
and provider-session lineage and continued from `correct`; they did not replay initial
implementation, adopt another run, advance the active checkout, or close the target task.

That run predates the durable attempt ledger. Its repeated correction cycles exposed the restart
accounting defect repaired by commit `8238383`. It is retained only as failure and recovery evidence:
it must not be retried and does not qualify the final supervised unit, pilot, or soak.

## Installed Package C

The release archive built from exact source commit `82383835d5f34dddac81e07b4394a7c5f91f62e4`
has SHA-256
`0e99a870836a7785118bc31fd1d62ef549aec62cf430152f8674819ecb5bd525`. Its build manifest identifies
the same source commit, declares Linux-only native evidence, and declares that credentials and
runtime state are absent.

The rootless installed binary has SHA-256
`e5c5986bef8b3a13a8cbfeec7567a4e4c3e664f4ab0ccd5db04b8fd60169e629`. Archive verification and the
installed `codingmage 0.1.0` version probe passed.

The installed binary passed the binary-level fixture
`supervised_restart_refuses_a_fourth_exact_correction_provider_attempt`. Across separate CLI
invocations, the fixture observed exactly three correction-provider process starts. The fourth
invocation returned `codingmage.provider.attempt_limit`, and the provider canary remained at three.
The test therefore observes the denial before provider spawn rather than inferring it from internal
state.

Unit verification also proved that a separately keyed immutable-review scope begins at attempt one
after the correction scope is exhausted. Mutating the persisted ledger causes integrity validation
to fail closed.

## Verification

The implementation checkpoint passed:

```bash
cargo fmt --all -- --check
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 scripts/docs_check.py
python3 -m unittest discover -s tests -p 'test_*.py'
git diff --check
```

The prescribed ten-outcome campaign passed. All 32 Python governance and evidence tests passed. Two
sustained qualification tests remained explicitly ignored because the controlled soak was not
authorized for that run; this evidence makes no soak claim.

The active downstream checkout remained at
`626a92146ff9769f83d36a599a813befb98c819c` with an empty porcelain-status SHA-256 of
`e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` during this reconciliation.

## Limitations

- Package C has not yet completed a fresh live supervised unit against the frozen target.
- The three-outcome unattended pilot and approximately ten-task controlled soak remain open.
- This evidence does not close Task `26.1.7`, `AC 26.8`, or any Sprint 26 gate.
- The older retained run remains diagnostic evidence only and is not valid final-candidate evidence.
