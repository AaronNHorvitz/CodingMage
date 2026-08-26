# Sprint 26 Deterministic Qualification Matrix

**Date:** 2026-08-26

**Source checkpoint:** `08bd1746befaa2f0e6d18ca961d5832901a1df4f`

**Scope:** Sub-task `26.1.7.2`

## Command Chain

The following fail-fast chain completed with exit status `0`:

```bash
cargo fmt --all -- --check && \
cargo test --workspace --all-targets --locked && \
cargo clippy --workspace --all-targets --locked -- -D warnings && \
python3 scripts/docs_check.py && \
python3 -m unittest discover -s tests -p 'test_*.py' && \
git diff --check
```

## Covered Outcomes

The matrix exercised the deterministic implementation for Tasks `26.1.4` through `26.1.6`:

- positive and exact-side-effect paths through supervised, serial, parallel five-pod, and
  prescribed ten-outcome campaigns;
- malformed, missing, duplicated, reordered, stale, broadened, cross-run, cross-task, and
  unsupported proposal, checkpoint, report, event, policy, and identity inputs;
- lower and upper resource boundaries, checked arithmetic, accepted-outcome ceilings, aggregate
  campaign limits, provider circuits, and per-stage provider-attempt limits;
- cancellation, provider timeout, process failure, hard exit, coordinator interruption, panic,
  watchdog, atomic-replace, uncertain effect, and restart recovery;
- repository, branch, worktree, task-source, active-checkout, unrelated-process, user-state, and
  owned-resource preservation;
- blocker continuation, dependency deferral, human-decision retention, no-progress detection,
  starvation prevention, deterministic replanning, and exact trigger observation; and
- serialized integration, stale-head refusal, cumulative validation, publication denial,
  destination-promotion denial, and cleanup/reconciliation.

The prescribed ten-outcome production-coordinator fixture passed in 193.95 seconds. The workflow
binary suite passed 13 tests; its two sustained qualification tests remained explicitly ignored
because they require `CODINGMAGE_SUSTAINED_SOAK=approved`. Those tests belong to the separately open
controlled-soak scope and are not substituted by this deterministic result.

All Rust unit, integration, binary, example, mutation, recovery, campaign, state, Git, process,
service, and controlled-target fixture tests passed. Strict workspace Clippy passed with warnings
denied. Documentation checks passed. All 32 Python governance, evidence, policy, traceability, and
privacy tests passed. Formatting and diff integrity passed.

## Disposition

Sub-task `26.1.7.2` is complete for its deterministic local matrix. This evidence does not claim a
successful live supervised unit, three-outcome live pilot, sustained soak, external platform,
manual fuzz, independent-review, signing, or publication result. Sub-tasks `26.1.7.3` through
`26.1.7.5`, `AC 26.8`, and every Sprint 26 gate remain open.
