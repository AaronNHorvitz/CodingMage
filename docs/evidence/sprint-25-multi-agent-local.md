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

The current implementation is verified with these local commands:

```bash
python3 -m unittest tests.test_multi_agent_matrix
cargo test -p codingmage-campaign --lib
cargo test -p codingmage-runtime --lib
cargo test -p codingmage-cli --test workflow parallel_campaign_runs_five_pods_and_serializes_reviewed_integrations -- --exact --ignored
cargo test -p codingmage-cli --test workflow serial_campaign_advances_two_reviewed_tasks_without_touching_active_checkout -- --exact
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
python3 scripts/docs_check.py
python3 -m unittest discover -s tests -p 'test_*.py'
git diff --check
```

The sustained five-pod qualification is separately guarded and ignored during ordinary test runs:

```bash
CODINGMAGE_SUSTAINED_SOAK=approved CODINGMAGE_SUSTAINED_SOAK_CYCLES=2 \
  cargo test -p codingmage-cli --test workflow sustained_five_pod_campaign_qualification \
  -- --exact --ignored --nocapture
```

Its checkbox and sustained-evidence gate remain open until that command is executed after the last
relevant reliability correction and the resulting duration, counts, resource growth, residue, and
source commit are recorded.

## External Qualification

[`qualify_campaign.py`](../../scripts/qualify_campaign.py) provides a guarded live runner. It
requires an exact acknowledgment, absolute ordinary files, a clean repository at the campaign's
immutable starting commit, deny-first destination policy, and mode-consistent publication. It runs
one read-only `doctor` preflight before one bounded campaign invocation and relies on the provider
CLIs' existing login stores; it does not read or persist credential values.

Authenticated provider, GitHub, CI, and integration evidence remains open until an operator supplies
an authorized disposable target and executes the guarded command. No ordinary unit, integration, or
CI command can cross that boundary accidentally.
