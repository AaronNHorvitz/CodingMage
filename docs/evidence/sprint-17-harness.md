# Sprint 17 Soak Harness Evidence

## Scope

This evidence covers the deterministic disposable-fixture and fault-schedule foundation for Sprint 17. It is not 24-hour or 48-hour wall-clock evidence and is not a controlled-target pilot result.

## Fixture Matrix

The harness materializes seven separate disposable Git repositories below a caller-provided new empty directory:

- Rust, Python, JavaScript, and documentation-only clean repositories.
- A repository with tracked and untracked user changes.
- A repository with a genuine unresolved merge conflict.
- A clean repository with a deliberately malformed task plan.

Fixture creation uses `/usr/bin/git` with an empty environment, disabled system/global configuration, disabled hooks, null standard input, fixed literal arguments, and repository-local synthetic identity. Linked, missing, non-directory, or nonempty fixture roots are rejected.

## Fault Schedule

The ordered scheduler covers Claude, Codex, GitHub, quota, network, sleep, agent crash, service restart, malformed output, stale commit, concurrent user change, pause/resume, and cancellation events. Empty, incomplete, unordered, duplicate, out-of-range, and unbounded campaigns fail validation.

An accelerated 10,000-cycle accounting run proves bounded retention and zero duplicate tasks, skipped gates, false completions, orphan processes, or unowned mutations. Each prohibited counter is independently mutated and blocks verification.

## Verification

Implementation commits: `b645495` and `41e376a`.

```text
cargo test -p codingmage-soak --all-targets
4 passed; 0 failed

cargo clippy -p codingmage-soak --all-targets --all-features -- -D warnings
passed

python3 scripts/check_architecture.py
passed

git diff --check
passed
```

## Open Evidence

Task `17.1.2`, both Sprint 17 acceptance criteria, and both sprint gates remain open. The required 24-hour campaign, corrected 48-hour campaign, real fault injection through composed provider/repository/service boundaries, supervised target units, unattended target story, and owner review have not been executed.
