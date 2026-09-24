# CM-R03 Reproducible Repair and Baseline-Bound Receipts - Local Evidence

- **Status:** Local implementation with deterministic tests only; no live provider or
  independent review is claimed.
- **Work package:** CM-R03 (CAP-10, 11, 14, 15, 31, 38), split in `TASKS.md`
- **Source revision:** the commit that carries this document

## CM-R03.1 - Baseline-bound gate receipts

Implemented in `crates/codingmage-runtime/src/gate_baseline.rs` and the workflow port's
`compare_with_baseline`:

- A candidate gate run that passes every configured gate is retained as the `GateBaseline` of
  its own commit and gate-registry digest under `state_root/gate-baselines/<repository_id>/`
  as an integrity-protected document. No extra process is ever run to obtain a baseline, so
  unit process counts and budgets are unchanged.
- Every candidate gate run is classified against the baseline of its base commit and retained as
  a `GateComparison`: `introduced` (failed now, passed at base), `pre_existing` (failed at both),
  `repaired` (passed now, failed at base) and `unclassified` (failed now, no baseline outcome).
  A base commit without a retained baseline yields `baseline = "unknown"`, which is an honest
  gap and never a pass. The comparison's integrity digest is appended to the unit's gate
  evidence, so the durable receipt binds the classification to the exact base and candidate.
- Behavior is otherwise unchanged: a failing required gate still fails the unit whether the
  failure is new or pre-existing. Refusing a unit whose base already fails a required gate, and
  an operator command that observes the baseline of a fresh head explicitly, are recorded as
  open under CM-R03.2.

Tests: `cargo test -p codingmage-runtime gate_baseline` (classification, unknown baseline,
identity and tamper refusal, store round trip); the supervised workflow test keeps its exact
process and utilization counts, proving no hidden gate run was added. The native workspace
now shows three gate evidence records per accepted unit with two configured gates: the two
gate receipts and the baseline comparison receipt (`codingmage-ui` changes test).

## Open

- CM-R03.2 to CM-R03.6 remain open as listed in `TASKS.md`; CM-R03.6 needs separately admitted
  disposable credentials and a target (External 3).
