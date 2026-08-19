# Sprint 19 Local Release Evidence

- **Status:** All currently implemented local gates pass; release review remains blocked
- **Source commit:** `bb17671`
- **Executed:** 2026-08-19 on Fedora Linux with Rust 1.95.0
- **Package SHA-256:** `8f2db488b2fc28a1d80706adb9a3ad02c8003999b1129b4c763d601e79b565f5`

## Passing Local Gates

- `cargo test --workspace --all-targets --quiet`: 165 declared Rust tests passed across all
  workspace targets, including hostile Git, process, recovery, provider, service, package, and
  AgentMage-shaped pilot fixtures.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- `python3 scripts/check_architecture.py`: passed, including explicit test-only dependency policy.
- `python3 scripts/docs_check.py`: passed.
- `python3 -m unittest discover -s tests -v`: 12 Python policy, mutation, documentation, secret,
  archive, and installer tests passed.
- `git diff --check`: passed.

The test-only dependency grant used by the cross-crate pilot is explicit and does not weaken the
production graph. A seeded architecture-policy test proves that changing the same edge to a normal
dependency is rejected.

## Reproducible Package

Two independent release invocations each performed two locked clean builds and produced identical
Linux archives. The archive contains the named binary, license and security documentation, SPDX
2.3 SBOM, build manifest, and checksums. An isolated prefix passed install, verify, version,
upgrade, rollback, verify, and remove. Package content scanning found no local user or repository
paths, named local user identifier, common API-key marker, or private-key header.

## Open Product Work

This evidence does not close sub-task `19.2.2.1` or Gate `19.1`. `codingmage run` still refuses
execution because the concrete production composition of the implemented ports is absent. The
package consequently cannot install and start a functional coordinator user service. Creating a
unit around the refusing command would produce false lifecycle evidence.

## External Evidence

No authenticated live Claude, Codex, or GitHub campaign; native macOS or Windows execution;
24-hour or 48-hour elapsed soak; supervised or unattended live AgentMage pilot; independent human
review; deferred manual fuzzing; release signature; or public release was performed or claimed.
