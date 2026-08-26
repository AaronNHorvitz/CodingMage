# Sprint 26 Autonomous Authority Evidence

## Boundary

This record covers the deterministic open-roadmap census, exact task-authority envelope, and
side-effect-free worktree planning required by Sub-tasks `26.1.4.1` through `26.1.4.3`. It does
not claim routine material-change classification (`26.1.4.4`), autonomous decomposition,
replanning, routing, watchdog completion, frozen-target qualification, release readiness, or any
external effect.

The implementation is split across source commit `0d6b6e0`, which introduced the closed census and
authority contracts and composed them into serial and team planning, and source commit `685e780`,
which moved exact worktree identity into a side-effect-free plan consumed by the production
workflow. The latter validates the envelope before claim acquisition, validates it again before
worktree creation and provider execution, and requires the resulting manifest to match the planned
worktree ID, run, task, path, and branch exactly.

## Verified Behavior

- Every open canonical sub-task receives exactly one closed readiness class in source order.
- Census identity binds the task source, campaign head, planning policy, local platform projection,
  provider-capability projection, dependencies, and content-free coordinator observations.
- Unknown, checked, malformed, contradictory, or provider-selected ready overrides fail closed.
- Each runnable unit receives an integrity-bound packet and envelope containing its exact canonical
  task and parent, source anchor and digest, base commit, planned worktree identity, dependencies,
  owned paths, acceptance criteria, literal gates, prohibited actions, risk digest, completion
  predicate, and independent technical limits.
- Cross-task, mutated, broadened, malformed, or stale envelopes fail closed.
- Planning a worktree creates no directory, manifest, branch, process, or Git mutation. Creation
  consumes that exact plan, and a changed plan is rejected before the first creation effect.
- Initial execution validates authority before acquiring its coordinator claim. Recovery validates
  a reconstructed plan against the trusted manifest before reacquiring its claim.

## Local Verification

The following commands passed against source commit `685e780` plus this evidence-only update:

```bash
cargo test -p codingmage-plan -p codingmage-codex -p codingmage-git -p codingmage-runtime --lib --locked
cargo clippy -p codingmage-plan -p codingmage-codex -p codingmage-git -p codingmage-runtime --all-targets --locked -- -D warnings
cargo fmt --all -- --check
python3 scripts/docs_check.py
python3 -m unittest discover -s tests -p 'test_*.py'
git diff --check
```

Focused coverage includes deterministic census construction and mutation rejection, cross-task
authority refusal, sealed envelope verification, exact readiness-digest binding in the lead packet,
side-effect-free worktree planning, mutated-plan refusal, exact manifest consumption, production
runtime lifecycle and recovery, and the existing hostile-repository preservation suite.

## Remaining Limitations

Sub-task `26.1.4.4` remains open because provider-reported changed paths alone are insufficient to
prove that a diff did not add a dependency, alter a public contract or accepted architecture,
weaken verification, or create an external effect. That classification must be derived from
coordinator-observed repository state and then composed into admission and review. No later Sprint
26 autonomous-progression or release gate is claimed by this record.
