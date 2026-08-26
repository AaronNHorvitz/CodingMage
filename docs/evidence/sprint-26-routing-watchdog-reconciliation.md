# Sprint 26 Routing, Watchdog, and Reconciliation Evidence

## Boundary

This record covers deterministic provider routing, exact watchdog and restart recovery, isolated
campaign-branch publication, terminal campaign reconciliation, and content-minimized operator
status for Task `26.1.6` and `AC 26.7`. It does not qualify the frozen target, installed package,
sustained soak, signing, manual fuzzing, independent review, native platforms, or release
publication.

Source commit `5b8b7cd` binds provider selection to declared task risk, ownership boundary,
configured capability, required strength, and exact prior failure count while retaining an
independent reviewer profile. Source commit `9af6a06` implements isolated-branch compare-and-swap
publication. Source commit `b88f935` adds exact watchdog observations and recovery-job equality.
Source commit `a8f77df` adds terminal ownership attestation. Source commit `6d0ae7f` closes the
parent-loss residue path through exact dead-guard and empty-target-group recovery.

## Verified Behavior

- Routine work uses the configured routine implementer and reviewer. High-risk or sensitive-path
  work and exact failure-history thresholds select only configured elevated profiles with the
  required strength. Missing strength fails closed, and implementer identity cannot satisfy the
  independent-review role.
- Every active task maps one-to-one to its exact durable lease and, when execution has begun, its
  exact implementation reservation. Orphaned, duplicate, or cross-task identities are refused.
  Closed watchdog states distinguish healthy, stale, expired, and idle observations.
- Restart recovery reconstructs the same task jobs and requires exact equality with the watchdog's
  recoverable task projection before provider execution. Journal intent recovery, provider session
  lineage, worktree identity, resource reservation, and lease identity remain load-bearing.
- The process guard binds its PID and kernel start identity before receiving the launch envelope.
  Parent loss terminates the exact target process group. Restart cleanup waits for that exact guard
  identity to disappear, proves the exact target group empty, refuses unexpected or symbolic
  entries, and removes only the selected private control directory.
- Isolated campaign publication observes the exact remote head, requires ancestry, uses an exact
  `--force-with-lease` compare-and-swap, reconciles uncertain transport outcomes by re-observation,
  and refuses divergence. It cannot promote or merge the protected destination branch.
- Final completion reloads and reconciles the append-only state journal, checks every exact campaign
  task in the canonical task source, requires all tasks merged with no active leases, reservations,
  or integration entries, loads every exact removed task-worktree manifest, inspects every exact
  owned process-control root, and binds those observations into the integrity-protected report.
- Campaign status schema version 5 reports content-minimized task, role, stage, blocker, retry,
  planning generation, checkpoint, watchdog, reconciliation, utilization, and stop-reason fields.
  Existing prohibited-content schema tests continue to reject prompts, source, provider prose,
  credentials, environment values, and hidden reasoning.
- Live concurrency, crash, recovery, cancellation, and hostile-state tests preserve unrelated
  processes, repositories, refs, worktrees, and destination branches.

## Local Verification

The following commands passed against source commit `6d0ae7f` plus this evidence-only update:

```bash
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
python3 scripts/docs_check.py
python3 -m unittest discover -s tests -p 'test_*.py'
git diff --check
```

Focused execution additionally covered all 105 runtime tests, all 13 live process tests, the
five-pod terminal-attestation workflow, local bare-remote publication, routing mutation and
boundary cases, interrupted execution reuse, provider retry ceilings, state-journal crash replay,
deadline cancellation, unrelated-process preservation, and zero-residue completion.

## Remaining Limitations

Task `26.1.7` must still run the frozen no-hardlinks qualification, fresh supervised unit,
three-outcome pilot, and approximately ten-task controlled soak. Tasks `26.2.1` through `26.2.3`
still own reproducible candidate packaging, installed-artifact qualification, operator-controlled
signing, manual fuzzing, and independent review. No external or release effect is claimed.
