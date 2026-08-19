# Sprint 1 Evidence

- **Status:** Passed
- **Source commit:** `7af9f2f42be260ef69b1a8bcd5e98e30914ce153`
- **Executed:** 2026-08-19 on Fedora Linux with Rust 1.95.0
- **Scope:** Workspace bootstrap, dependency direction, identifiers, and public errors

## Commands

The following commands ran from a clean checkout of the source commit and exited successfully:

```bash
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps
python3 scripts/check_architecture.py
python3 scripts/docs_check.py
python3 -m unittest discover -s tests -p 'test_*.py'
git diff --check
```

Eight Rust tests passed across the contracts and core crates. Eight Python tests passed, including
a seeded forbidden dependency edge that named `codingmage-contracts -> codingmage-core` exactly.

## Contract Findings

- Eight identifier types enforce one canonical ASCII grammar and reject empty, oversized,
  path-bearing, control-character, ambiguous, and noncanonical values before construction.
- Public errors contain only a validated stable code and typed identities or numeric bounds. Their
  public schema has no free-form command, source, environment, or credential field.
- Unknown but syntactically valid future error reasons round-trip, while unknown fields and invalid
  error categories fail closed.

## Limitations

This evidence establishes contracts only. It does not claim configuration loading, repository
authorization, Git operations, process execution, provider integration, or orchestration behavior.
