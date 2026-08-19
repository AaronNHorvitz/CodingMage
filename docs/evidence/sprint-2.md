# Sprint 2 Evidence

- **Status:** Passed on the Linux reference platform
- **Source commit:** `19573f6c2ad2cdeb5b9fd98f5fdfeb8e0413f38e`
- **Executed:** 2026-08-19 on Fedora Linux with Rust 1.95.0
- **Scope:** Configuration loading and repository authorization

## Commands

The complete formatting, 21-test Rust workspace, strict Clippy, rustdoc, architecture,
documentation, eight-test Python mutation suite, and `git diff --check` gates ran from a clean
checkout and exited successfully.

## Verified Behavior

- Configuration is loaded only from an explicit nonsymlink file, rejects unknown and duplicate
  fields, and uses typed deny-by-default grants instead of implicit booleans.
- Conflicting network, push, issue, pull-request, and publication policies fail before authority is
  granted. The effective diagnostic view omits raw paths, branch names, models, executables, and
  command arguments.
- Repository authorization holds open handles to the target and Git metadata directories and
  records filesystem identity, canonical path, initial `HEAD`, and SHA-256 remote URL fingerprints.
- Bare, nested, symlinked, physically aliased, overlapping, self-targeted, unsupported-owner,
  renamed, replaced, and stale-`HEAD` fixtures fail closed with stable repository error codes.

## Limitations

- Native ownership evidence is Linux-only. Non-Linux adapters remain unavailable and unclaimed.
- The bind-alias test injects equal physical identities into the same overlap predicate used by
  authorization; it does not require or perform a privileged host bind mount.
- No Git write, worktree, process, network, provider, or orchestration operation is implemented or
  claimed by this sprint.
