# Sprint 21 Gate Evidence

- **Status:** Complete
- **Gate implementation commit:** `5813f5b1daae73f9e8a2ab88875105ffd769dab7`
- **Executed:** 2026-08-21 on Fedora Linux with Rust 1.95.0

## Authority Matrix

The canonical campaign-authority test changes every mutable field independently: campaign and
repository identity, repository path, initial commit, task-source and operator-authorization
digests, pod and unit ceilings, all three provider profiles, implementation authentication, gate
tiers, campaign branch, allowed and denied paths, protected branches, and publication mode. Every
valid mutation produces a different canonical authority digest. Invalid version and unknown
publication variants fail closed. Existing hostile-spec tests cover relative paths, overlap,
unbounded ceilings, malformed identities, and protected-branch contradictions.

The proposal corpus covers stale task-source identity, non-ready and duplicate tasks, contradictory
dependencies, escaping and denied paths, artifacts outside owned roots, unknown gates, unsupported
authority fields, proposal mutation after sealing, and deterministic repeatability across 32
identical validations.

## Disposition Matrix

The closed-outcome matrix executes all four dispositions and every reason family:

- seven blocker reasons, with `blocked_prerequisite` correctly refused for an already
  dependency-ready task;
- six deferral reasons against all six reconsideration triggers, accepting only the required
  one-to-one pair;
- all six human-decision reasons; and
- all 12 ways one valid disposition can carry another disposition's payload.

Runtime and binary evidence adds durable blocker continuation and clearance, exact external-trigger
observation, repeated satisfied-deferral escalation, the 1,296-case starvation matrix, durable
human-decision continuation, malformed and unauthorized output refusal, restart behavior, and
zero downstream effect assertions.

## Verification

```text
cargo test -p codingmage-campaign --all-targets --locked
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
cargo doc --workspace --no-deps --locked
python3 scripts/docs_check.py
python3 scripts/check_architecture.py
python3 -m unittest discover -s tests -p 'test_*.py'
git diff --check
```

All local commands passed. Gates `21.1` and `21.2` are complete. This evidence does not claim an
external human review, native non-Linux execution, authenticated network Git, or deferred manual
fuzzing.
