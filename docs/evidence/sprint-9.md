# Sprint 9 Evidence

- **Status:** Passed locally
- **Source commit:** `13fae69a94687c4ffc5d68ec0120c552711da30e`
- **Executed:** 2026-08-19 on Fedora Linux with Rust 1.95.0
- **Scope:** Deterministic risk classification, model routing, escalation, capacity, and overrides

## Commands

The 81-test Rust workspace, formatting, strict Clippy, rustdoc, architecture policy,
documentation checks, eight Python mutation tests, and `git diff --check` passed. Seven focused
routing tests exercise the fixed policy corpus.

## Verified Behavior

- Classification consumes trusted labels, changed paths, dependency breadth, changed file and line
  counts, prior failures, unresolved findings, final-gate and dispute state, and content-free
  performance feedback.
- Security, authentication, credentials, cryptography, concurrency, process control, Git mutation,
  persistence, cross-platform, packaging, release, and architecture signals route at critical
  strength. Unknown signals raise routine work to at least elevated risk.
- Routine implementation selects Claude Sonnet and elevated or repeatedly failing implementation
  selects Claude Opus. Routine review selects Codex Terra High; critical, disputed, and final
  review selects Codex Sol High. Mechanical administration selects deterministic local code.
- Decisions retain provider, role, profile, effort, speed, risk, sorted reason codes, escalation
  conditions, optional operator reason, and exact resolved model identity when safely exposed.
- Missing configured profiles, operational unavailability, and quota limits return distinct stop
  errors without selecting a weaker fallback.
- Operator pins require a stable reason, stay within the required provider, and may preserve or
  raise strength but never weaken a mandatory decision. Final review cannot be pinned below Sol.
- Unknown usage remains absent rather than becoming zero. Usage pressure does not change final-gate
  strength.

## Limitations

- Profile labels are policy identities. Provider adapters must record the exact resolved provider
  model when exposed and reconcile it before relying on the decision.
- Capacity observations are supplied by provider and scheduler layers implemented in later sprints;
  the routing layer deterministically enforces their normalized state.
