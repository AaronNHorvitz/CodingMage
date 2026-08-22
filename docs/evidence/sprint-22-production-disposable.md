# Sprint 22 Production Disposable Evidence

- **Status:** Production disposable qualification passes; controlled-target qualification remains
  open
- **Accepted-outcome correction commit:** `518d4e6`
- **Production fixture commit:** `918b560`
- **Final residue-assertion commit:** `c8656c2`
- **Executed:** 2026-08-21 on Fedora Linux with Rust 1.95.0

## Authority And Baseline

The binary-level `prescribed_campaign` fixture imports and validates the immutable ten-outcome
schedule from `codingmage-soak`, then runs the production serial campaign coordinator against a
new disposable Git repository. The authority fixes one pod, `local_only` publication, an accepted
outcome ceiling of ten, fixed implementer, lead, and reviewer profiles, one exact gate tier, a
protected default branch, and `src` as the only writable path family.

Before provider work, the fixture records content-free repository and task-source identities from
the production doctor result. The coordinator validates the configuration and provider capability
surfaces before mutation, owns private state and scratch roots, and accounts for process
invocations, output, retained storage, and elapsed execution. The test retains the original active
checkout head, status, task source, and baseline file for final comparison. No prompt, source body,
provider prose, command output, credential, or unrestricted environment value is persisted as
qualification evidence.

## Prescribed Outcomes

The production campaign reaches exactly ten accepted outcomes:

1. One clean reviewed completion.
2. One completion after a deterministic gate correction.
3. One completion after an independent review correction.
4. One typed unavailable-external-dependency blocker.
5. One typed temporary deferral bound to an operator-resume trigger.
6. One completion after a bounded malformed-report repair.
7. One completion after a typed provider-capacity pause and retry.
8. One completion after interruption at durable integration intent and restart without
   implementation replay.
9. One completion followed by the exact stop-after-unit boundary.
10. The deferred task's completion after explicit trigger observation, followed by exact
    accepted-outcome ceiling enforcement.

The final projection contains eight completed outcomes, one blocked outcome, no pending deferral,
one satisfied deferral history, and ten accepted outcomes. The satisfied deferral remains an
accepted historical outcome after its task later completes. Rejected proposals are not counted.

## Reconciliation

The fixture verifies per-task lead and implementer call counts, one malformed-report repair, two
correction rounds, nonzero provider and process observations, and nonzero retained-state
accounting. The interrupted task is implemented exactly once. The capacity-paused task is selected
twice, while the external blocker is selected once and is not replayed.

Eight exact task checkboxes are closed on the isolated campaign branch. The blocked task remains
open. The active checkout head, clean status, task source, and unrelated baseline file remain
byte-for-byte unchanged. Exactly two Git worktrees remain at the recoverable ceiling stop: the
untouched active checkout and the owned campaign-root worktree. No pod worktree remains.

## Verification

The final implementation-bound repetition passed:

```text
cargo test -p codingmage-cli --test prescribed_campaign --locked -- --nocapture
```

Result: one passed, zero failed, completed in 168.81 seconds at commit `c8656c2`.

The focused compile, lint, formatting, and repository checks also pass:

```text
cargo test -p codingmage-cli --test prescribed_campaign --locked --no-run
cargo clippy -p codingmage-cli --test prescribed_campaign --locked -- -D warnings
cargo fmt --all -- --check
git diff --check
```

Earlier runs are not treated as current evidence. The first post-fixture rerun exposed an incorrect
expectation that all scratch state should disappear at a recoverable ceiling pause. The corrected
test instead requires the one owned campaign-root worktree and rejects every leaked pod worktree.
The passing result above was generated only after that correction was committed.

## Open Boundary

This evidence closes only disposable production qualification in Task 22.3.1. It does not claim a
controlled-target campaign, human cumulative-diff review, publication, concurrency expansion, or
Sprint 22 Gate 22.3. Tasks 22.3.2 and 22.3.3 remain open.
