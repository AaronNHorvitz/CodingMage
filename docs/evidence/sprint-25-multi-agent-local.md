# Sprint 25 Multi-Agent Local Evidence

## Boundary

This record covers deterministic local, fake-provider, fake-publication, and local-process evidence
for the multi-agent campaign implementation. It does not claim authenticated provider or GitHub
qualification, sustained-duration execution, native macOS or Windows evidence, independent human
review, package signing, release publication, or valuable-target approval.

The machine-checkable mapping is
[`multi-agent-scenario-matrix.json`](multi-agent-scenario-matrix.json). Every required scenario maps
to existing implementation and executable test symbols. The matrix validator rejects missing,
duplicate, reordered, malformed, or dangling mappings and independently scans tracked content for
the prohibited private project identifier.

## Local Commands

The current implementation was verified against source and task-ledger baseline `5d61bf3` with
these local commands:

```bash
python3 -m unittest tests.test_multi_agent_matrix
cargo test -p codingmage-campaign --lib
cargo test -p codingmage-runtime --lib
cargo test -p codingmage-cli --test workflow parallel_campaign_runs_five_pods_and_serializes_reviewed_integrations -- --exact --nocapture
cargo test -p codingmage-cli --test workflow serial_campaign_advances_two_reviewed_tasks_without_touching_active_checkout -- --exact
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
python3 scripts/docs_check.py
python3 -m unittest discover -s tests -p 'test_*.py'
git diff --check
```

## Results

| Command | Exit | Result |
| --- | --- | --- |
| `cargo test --workspace --all-targets` | `0` | 323 passed, 0 failed, 1 ignored guarded qualification. |
| Exact process-backed five-pod workflow | `0` | 1 passed, 0 failed, 60.98 seconds. |
| Guarded sustained five-pod workflow | `0` | 1 passed, 0 failed, 2 complete internal cycles, 121.97 seconds. |
| `cargo clippy --workspace --all-targets -- -D warnings` | `0` | No warnings. |
| `cargo fmt --all -- --check` | `0` | No formatting drift. |
| `python3 -m unittest discover -s tests -p 'test_*.py'` | `0` | 19 passed, 0 failed. |
| `python3 -m unittest tests.test_multi_agent_matrix` | `0` | 3 passed, 0 failed. |
| `python3 scripts/docs_check.py` | `0` | Documentation checks passed. |
| `cargo test -p codingmage-plan --lib` | `0` | 10 passed, including canonical repository-plan parsing. |
| `git diff --check` | `0` | No whitespace errors. |

The scenario-matrix SHA-256 at this baseline is
`02961fdbbae9995c350c1def63a2fb50f7e1a5399bb0611a319f01cb11cee499`. No release artifact,
package, SBOM, signature, or publication asset was produced by these local implementation gates, so
no release-artifact digest is applicable.

The sustained five-pod qualification is separately guarded and ignored during ordinary test runs:

```bash
CODINGMAGE_SUSTAINED_SOAK=approved CODINGMAGE_SUSTAINED_SOAK_CYCLES=2 \
  cargo test -p codingmage-cli --test workflow sustained_five_pod_campaign_qualification \
  -- --exact --ignored --nocapture
```

The guarded command passed at its minimum two-cycle bound after the last local concurrency test
correction. This is current local multi-pod soak evidence, not a claim of long-duration,
authenticated-provider, native-platform, independently reviewed, or valuable-target qualification.
A sustained one-pod production run remains open, so the combined sustained rollout gate remains
open.

## External Qualification

[`qualify_campaign.py`](../../scripts/qualify_campaign.py) provides a guarded live runner. It
requires an exact acknowledgment, absolute ordinary files, a clean repository at the campaign's
immutable starting commit, deny-first destination policy, and mode-consistent publication. It runs
one read-only `doctor` preflight before one bounded campaign invocation and relies on the provider
CLIs' existing login stores; it does not read or persist credential values.

Authenticated provider, GitHub, CI, and integration evidence remains open until an operator supplies
an authorized disposable target and executes the guarded command. No ordinary unit, integration, or
CI command can cross that boundary accidentally.
