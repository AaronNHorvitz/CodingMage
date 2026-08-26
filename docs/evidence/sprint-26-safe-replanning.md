# Sprint 26 Safe Replanning Evidence

## Boundary

This record covers exact blocker projection, continued independent selection, immutable serial
planning generations, bounded no-progress detection, and decomposition replay refusal for
Sub-tasks `26.1.5.2`, `26.1.5.3`, and `26.1.5.4`. It does not close Sub-task `26.1.5.1`, Task
`26.1.5`, or `AC 26.6`: sealed child plans are durable, but live campaign execution does not yet
consume them.

Source commit `5afb82c` introduced the chained planning-generation checkpoint and replay-safe
decomposition persistence. Source commit `c670b56` added the explicit blocked-prerequisite and
independent-work regression. Source commit `61f26a6` separated logical stagnation from bounded
provider recovery and added the end-to-end no-progress campaign assertion.

## Verified Behavior

- An exact blocked task is projected unavailable without changing its canonical checkbox.
- Dependency descendants remain open and cannot become ready while their blocked prerequisite is
  incomplete; independent dependency-ready work remains selectable.
- Every serial planning invocation appends an integrity-bound generation containing its exact
  campaign head, task-source digest, readiness-census digest, sorted ready-task identities,
  state fingerprint, trigger set, sequence, prior-generation digest, and body digest.
- Initial planning, completion, blocker, deferral, satisfied deferral, rejected proposal,
  recoverable failure, dependency change, integration, and human-decision paths schedule closed
  planning triggers. Head or task-source changes add a dependency-change trigger from observed
  state rather than provider assertion.
- Two consecutive unchanged replans are retained as evidence and stop before another model
  invocation with `codingmage.campaign.no_progress_limit`.
- Duplicate blockers fail closed, repeated satisfied deferrals become explicit human decisions,
  material decisions remain outside routine authority, and model output cannot mint task
  authority.
- A persisted decomposition can be replayed idempotently only when byte-identical. A different
  valid decomposition for the same parent is refused rather than replacing the sealed plan.
- Checkpoint schema version 8 rejects older or mutated generation ledgers and preserves the exact
  digest chain across restart.

## Local Verification

The following commands passed against source commit `61f26a6` plus this evidence-only update:

```bash
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
python3 scripts/docs_check.py
python3 -m unittest discover -s tests -p 'test_*.py'
git diff --check
```

Focused coverage includes generation chaining, trigger persistence, dependency-change inference,
restart reload, no-progress cutoff, decomposition replay refusal, duplicate disposition refusal,
satisfied-deferral escalation, exact blocker preservation, descendant suppression, and continued
independent selection.

## Remaining Limitations

The runtime can build, verify, persist, reload, and reject replay of sealed decompositions, but the
serial campaign does not yet dispatch their child packets. Sub-task `26.1.5.1`, Task `26.1.5`, and
`AC 26.6` therefore remain open. No routing, watchdog, release-candidate, installed-package,
external-review, signing, native-platform, publication, or manual-fuzzing claim is made here.
