# Sprint 20 Correction Recovery Evidence

- **Status:** Local deterministic correction-loop gate complete; authorized live-provider gate open
- **Implementation commit:** `c9421a44fafa976e961784f1d5b184e117884c4f`
- **Executed:** 2026-08-21 on Fedora Linux with Rust 1.95.0

## Implemented Boundary

Before a correction can invoke Claude or mutate Git, CodingMage writes a private, atomically
replaced, SHA-256-bound checkpoint containing the repository, run, task, worktree, branch, source
commit, candidate parent, exact Claude session, and correction round. It stores no prompt,
diagnostic, source text, review prose, credential, or provider transcript.

Campaign restart retains and reuses the exact run identity. It reloads the manifest-selected owned
worktree, reconstructs the immutable pre-correction candidate, and enters the coordinator only at
the interrupted correction. It does not repeat lead planning, claim acquisition, worktree creation,
initial implementation, or the initial candidate commit.

If the exact Claude session exists, CodingMage resumes it. If Claude positively reports that the
session does not exist, CodingMage starts the same precommitted session identity once. If the
coordinator commit already exists, CodingMage contacts no provider and performs no Git mutation: it
accepts only a clean direct child with the fixed coordinator author, email, task-bound message, and
leased changed paths. Every mismatch fails closed.

## Verification

The binary crash fixture interrupts after correction identity and durable intent but before provider
execution. Restart produces exactly these implementer observations: one original implementation,
one missing-session reobservation, and one correction start. The resumed invocation performs no
campaign-lead or initial-implementation call, passes corrected gates, receives fresh exact-commit
review, reconciles the task, and leaves the active checkout unchanged.

The coordinator fixture proves that recovery records one correction intent and one observation,
does not replay claim, worktree creation, or implementation, and continues through local gates,
review, final verification, checkpoint, completion, and release. Git tests accept the exact
coordinator child and reject wrong parent, unleased paths, dirty committed state, and noncoordinator
provenance. Checkpoint tests cover integrity mutation and every authority-bearing identity.

The following gates passed from the implementation tree:

```text
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

## Open Evidence

`Gate 20.2` remains open because this evidence uses deterministic process-backed provider fixtures,
not an authorized live Claude-and-Codex correction. Initial implementation-session interruption is
also outside this unit and remains open. No unattended-release, remote-publication, macOS, Windows,
or external-review claim is made.
