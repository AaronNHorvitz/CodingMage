# Sprint 26 Correction Timeout Hardening Evidence

**Date:** 2026-08-25

**Implementation commits:** `b5d06ce999e1a337dc396a42e341e17d8350729c`, `300822f8c83ee5144150848a15227c2eae9f96dc`, `9b21b2f32012bf04a8b71a83c0087f6555aba2f6`

**Scope:** Sub-tasks `26.1.3.1` through `26.1.3.5`, `26.1.3.7`, and `26.1.3.8`

## Claim

CodingMage now distinguishes a bounded correction-provider timeout from generic provider failure and attempt-limit exhaustion. While campaign authority remains available, it may resume only the latest integrity-verified prepared correction with the same run, worktree, parent commit, correction round, and provider-session lineage. Repeated timeouts stop with `provider_timeout` and blocker `codingmage.campaign.unit_provider_timeout`.

The durable checkpoint stores only a closed diagnostic kind, the total item count, at most four bounded item identities, and a SHA-256 digest. It excludes reviewer prose, repository content, command output, credentials, and model reasoning. Correction prompts limit each increment to four explicit findings and prohibit speculative redesign, new dependencies, and temporary probe artifacts.

CodingMage also preserves an integrity-verified initial candidate when a fresh read-only senior review returns a transient provider, thread, or timeout failure. A bounded retry keeps the same run and owned worktree, validates the initial checkpoint and candidate identity, reruns deterministic gates, and starts a fresh review against the same immutable commit. It does not replay the implementation provider. A malformed correction checkpoint prevents retention rather than falling through to initial-candidate recovery.

Provider retry accounting is scoped by durable stage identity. Initial implementation, one exact correction checkpoint, and one exact immutable review candidate each retain an independent three-attempt ceiling. Correction scope binds run, round, and parent commit; review scope binds run and candidate commit. Moving to another scope resets only that local three-attempt counter. Persisted aggregate provider-attempt, correction-round, process, output, retained-state, and elapsed-time limits remain unchanged and continue to cap the complete campaign.

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

The binary-level `serial_campaign_scopes_provider_retries_without_replaying_implementation` fixture additionally proved one failed initial provider call, one successful implementation, two failed senior reviews, a third successful review bound to the identical candidate commit, deterministic gates before every review, final integration verification, one completed isolated campaign unit, and byte-for-byte preservation of the active checkout and task source.

## Proven Boundaries

- Each provider invocation retains the existing 15-minute deadline.
- Retry is available only for an existing integrity-verified correction checkpoint.
- Campaign retry remains bounded by the existing three-attempt ceiling.
- The active checkout and task source remain outside candidate-worktree mutation.
- Terminal timeout produces no completion commit and does not mark the target task complete.
- Owned candidate state remains available for exact recovery; unrelated repository state is not adopted.
- Transient initial-candidate review recovery applies only to provider, thread, and timeout failures after a valid `CandidateObserved` checkpoint exists.
- Each retry revalidates the durable identities and immutable candidate and reruns gates before opening a fresh read-only reviewer session.
- Implementer provider failures without a valid correction checkpoint continue to restart the unit with a fresh run rather than adopting partial state.
- One stage cannot consume another stage's local retry allowance, but all attempts still count against persisted aggregate campaign limits.

## Limitations

This record does not claim that a fresh live controlled-target campaign has passed. That claim remains open under Sub-task `26.1.3.6`. It also does not close the separately authorized sustained-soak, manual-fuzz, independent-review, package-signing, or external-platform gates.

## Installed Candidate and Live Qualification Attempt

After the implementation and evidence commits, the documented packager produced two matching locked release builds and installed the resulting archive through the rootless installer. Verification observed:

- packaged source commit `3c2ac78e34684fa998be79798ce02191ea946639`;
- archive SHA-256 `882c7deccd3d510064a6b13b2b205199ed7cea526c52a29c7eefd288a4d200d8`;
- installed binary SHA-256 `7f12a9b512d1de47b2cb7e0d5a697db8cf9e73ff7a835a01a26346da030b4c53`;
- `python3 scripts/install_release.py verify` and the installed `codingmage 0.1.0` version probe passed; and
- the build manifest declared Linux-only evidence with credentials and runtime state absent.

The installed candidate then passed `doctor` against a clean, exact controlled-target checkout with network, push, issues, pull requests, task merge, destination merge, and publication effects denied. Its supervised unit produced an isolated candidate, repaired one deterministic-gate finding, passed the complete configured gates, and reached independent immutable review. The subsequent review-correction provider invocation returned the closed local provider result `authentication_failed`; an exact retained-state rerun returned the same result before another repository effect. A separate fresh-state attempt using the newer installed provider executable also returned `authentication_failed` with zero input and output tokens.

The active checkout remained clean at its original head, its task stayed open, and no controlled-target branch was integrated or published. The three-outcome unattended pilot was therefore not started. Reauthentication is an external operator prerequisite, so Sub-task `26.1.3.6`, Task `26.1.3`, and `AC 26.4` remain open.

## Authenticated Reviewer-Recovery Qualification

After provider authentication was restored, the package from source commit `df6ae38f77fd24fc59ad261cb27c939be44bc385` installed and verified with archive SHA-256 `fed7f927f2c47581e83437af3ebb9e6cd479d28cb0f1828a89986771283867c9` and installed-binary SHA-256 `749f0c2a51d3d97da37c1a72104a7d43cf638e2017b49f9af8f3e504976d2724`.

A fresh supervised `1.2.1.2` unit completed initial implementation, repaired a deterministic-gate finding, passed all configured gates, received a valid changes-required senior review, and then stopped safely when the review-correction provider returned a transient failure. The active checkout remained unchanged and the task remained open.

Two byte-identical source-free campaign preflights then passed with SHA-256 `2ccc9fe1710305738ecbc85b53f241d6f23e881cc4ca35dac973247c449bf007`. The fresh one-pod local-only campaign observed:

- one transient initial-implementation failure followed by a clean whole-unit restart;
- one valid candidate after the second implementation, two deterministic-gate corrections, and a fully green gate set;
- one transient senior-review failure after the immutable candidate was green;
- retention and integrity revalidation of the same run, worktree, correction lineage, and candidate without implementation replay;
- a complete deterministic-gate rerun followed by a fresh read-only senior review; and
- a second transient senior-review failure, after which the campaign paused with `attempt_limit` and `codingmage.campaign.provider_unavailable`, zero accepted units, unchanged campaign head, unchanged active checkout, and an open task.

That run proved Sub-task `26.1.3.7` but also showed that one initial-stage failure consumed one of the unit-wide three provider attempts, leaving only two review attempts. Commit `9b21b2f32012bf04a8b71a83c0087f6555aba2f6` corrects that limitation with checkpoint-scoped local ceilings while preserving aggregate campaign limits. A new installed-candidate campaign remains required before Sub-task `26.1.3.6` can close.
