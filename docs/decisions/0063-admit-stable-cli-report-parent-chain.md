# ADR 0063: Admit a Stable Parent Chain for CLI Report Publication

- **Status:** Accepted for local correction; independent re-review pending
- **Date:** 2026-10-01
- **Decision owners:** CodingMage implementation

## Context

The independent exact-commit review of `b8e8458416c3ec426f94624306a6f8d479e3e5d6`
found that a checked destination parent could be renamed between its last
name check and a descriptor-relative `linkat` or `renameat2`. The accepted
report would then appear in the moved directory before the post-publication
check returned uncertainty. An error after publication cannot protect the
report's destination or privacy.

## Decision

Before candidate creation, admit only a canonical destination parent whose
entire directory chain is owned by the effective user or root. Each ancestor
that is group or other writable must have the sticky bit, protecting the next
entry owned by the effective user or root from another unprivileged UID. The
destination parent itself must deny group and other writes, including when it
has the sticky bit, because an existing public leaf could belong to another
UID. Refuse any symlink or missing component encountered in the resolved
canonical chain. Recheck the held parent's current chain before publication.
Keep the existing exact requested-name, repository, mount, candidate and
displaced-inode checks.

This admission makes a rename of an accepted parent by a different
unprivileged UID unavailable between the final check and publication. A
process running as the same UID, or a privileged actor able to change the
chain, remains outside this ownership boundary; no separate pathname check
can be atomic with either publication syscall. The command does not claim to
protect against those actors. No new package or licence is introduced.

## Consequences

A requested report directly in a shared writable directory, or under an
ancestor writable by other UIDs without sticky protection, is refused before
the report candidate is made. An owner-controlled directory below a sticky
shared parent remains usable. Deterministic hooks move the parent immediately
before both publication syscalls and prove the final check refuses without
writing report bytes. The earlier independent finding remains recorded; this
decision requires a fresh exact-commit review.

The public Setup command, bounded native worker and complete Show-command
coverage under Task 36.3.2.2 remain open, as do all human, live and release
gates.

## Verification

See [the CLI report writer evidence](../evidence/sprint-36-cli-report-writer.md)
for focused tests, cumulative checks and the independent disposition.
