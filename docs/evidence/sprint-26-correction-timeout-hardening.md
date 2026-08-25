# Sprint 26 Correction Timeout Hardening Evidence

**Date:** 2026-08-25

**Implementation commit:** `b5d06ce999e1a337dc396a42e341e17d8350729c`

**Scope:** Sub-tasks `26.1.3.1` through `26.1.3.5`

## Claim

CodingMage now distinguishes a bounded correction-provider timeout from generic provider failure and attempt-limit exhaustion. While campaign authority remains available, it may resume only the latest integrity-verified prepared correction with the same run, worktree, parent commit, correction round, and provider-session lineage. Repeated timeouts stop with `provider_timeout` and blocker `codingmage.campaign.unit_provider_timeout`.

The durable checkpoint stores only a closed diagnostic kind, the total item count, at most four bounded item identities, and a SHA-256 digest. It excludes reviewer prose, repository content, command output, credentials, and model reasoning. Correction prompts limit each increment to four explicit findings and prohibit speculative redesign, new dependencies, and temporary probe artifacts.

## Verification

The following fail-fast command chain completed with exit status `0` from the implementation commit plus the evidence-only reconciliation changes:

```bash
cargo fmt --all -- --check && \
cargo test --workspace --all-targets --locked && \
cargo clippy --workspace --all-targets --locked -- -D warnings && \
python3 scripts/docs_check.py && \
python3 -m unittest discover -s tests -p 'test_*.py' && \
git diff --check
```

Observed results:

- All Rust workspace unit, integration, workflow, recovery, controlled-target, and all-target tests passed.
- Two sustained qualification tests remained explicitly ignored because they require `CODINGMAGE_SUSTAINED_SOAK=approved`; no soak claim is made here.
- The prescribed ten-outcome coordinator test passed.
- Strict workspace Clippy passed with warnings denied.
- Documentation checks passed.
- All 32 Python governance and evidence tests passed.
- Diff integrity passed.

Focused tests also passed for provider-timeout mapping, correction-checkpoint round trips and mutation refusal, content minimization, exact correction resume without implementation replay, and terminal classification after three repeated correction timeouts.

## Proven Boundaries

- Each provider invocation retains the existing 15-minute deadline.
- Retry is available only for an existing integrity-verified correction checkpoint.
- Campaign retry remains bounded by the existing three-attempt ceiling.
- The active checkout and task source remain outside candidate-worktree mutation.
- Terminal timeout produces no completion commit and does not mark the target task complete.
- Owned candidate state remains available for exact recovery; unrelated repository state is not adopted.

## Limitations

This record does not claim that the rebuilt installed binary or a fresh live controlled-target campaign has passed. Those claims remain open under Sub-task `26.1.3.6`. It also does not close the separately authorized sustained-soak, manual-fuzz, independent-review, package-signing, or external-platform gates.
