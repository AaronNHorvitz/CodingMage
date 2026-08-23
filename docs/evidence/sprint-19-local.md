# Sprint 19 Local Release Evidence

- **Status:** Current local gates and the packaged Fedora service lifecycle pass; release review remains blocked
- **Source commit:** `07304426c457f0a6c4a820784f4e6f00c20824e9`
- **Executed:** 2026-08-23 on Fedora Linux
- **Package SHA-256:** `010de791152c61fa7956cbf999c0292abac542307e1de980a4f3dfd452f1c58e`

## Passing Local Gates

- `cargo test --workspace --all-targets --locked --offline`: passed across all workspace targets;
  the two explicitly guarded qualification tests remained ignored by the ordinary suite.
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- `python3 scripts/check_architecture.py`: passed, including explicit test-only dependency policy.
- `python3 scripts/docs_check.py`: passed.
- `python3 scripts/verification_inventory.py`: passed with 1,109 inventoried surfaces and 866
  explicit category gaps. The gaps remain open work rather than implied coverage.
- `python3 -m unittest discover -s tests -p 'test_*.py'`: 32 Python policy, mutation,
  documentation, secret, archive, inventory, and installer tests passed.
- `git diff --check`: passed.

The test-only dependency grant used by the cross-crate pilot is explicit and does not weaken the
production graph. A seeded architecture-policy test proves that changing the same edge to a normal
dependency is rejected.

## Reproducible Package

The clean release invocation produced a Linux archive bound to source commit `0730442`. Its closed
build manifest records version `0.1.0`, the source epoch, Cargo lock digest, and binary digest; it
also declares `contains_credentials: false`, `contains_runtime_state: false`, and Linux-only native
evidence. The hardened installer rejected undeclared archive contents and invalid manifest fields
in mutation tests. A fresh isolated prefix passed install, verify, version, and remove.

Separately, the packaged Fedora `systemd --user` lifecycle passed install, native unit verification,
start, expected restart behavior with an inert synthetic campaign, stop, upgrade, rollback, and
removal. The unit and receipt were absent after cleanup. That lifecycle is recorded in the Sprint
18 evidence and does not claim that a full installed provider campaign completed.

## Open Product Work

This evidence closes only sub-task `19.2.2.1`. It does not close Task `19.2.2` or Gate `19.1`.
Independent review, explicit open-risk disposition, a signed candidate, the installed-artifact
full campaign workflow, deferred manual fuzzing, and the 866 explicit verification-category gaps
remain unresolved or separately gated.

## External Evidence

The controlled local Claude/Codex target campaign stopped truthfully at the repository-boundary
validator and produced no accepted outcome. No authenticated GitHub campaign, native macOS or
Windows execution, logout/shutdown endurance evidence, independent human review, deferred manual
fuzzing, release signature, or public release was performed or claimed.
