# Decision 0045: Report Export Mount Boundary

- **Status:** Accepted for the CLI and native report writers
- **Date:** 2026-09-30
- **Decision owners:** CodingMage implementation lane under Decision 0020
- **Supersedes:** The path-only containment check in Decisions 0038 and 0044
- **Superseded by:** None

## Context

An independent review of commit `0ec26baa39bd730e551227ee6dfbaeec2b233578`
found that Linux bind mounts can expose a repository directory through another
absolute path. Resolving symlinks and comparing path prefixes does not detect that
alias. Both the public CLI writer and the native helper used that path-only check.

## Decision

Both writers now hold a directory descriptor for the authorized repository root as
well as the destination parent. They read the kernel mount ID for each descriptor
and for the currently requested repository and destination paths during every
retained-handle check. Publication is refused unless all four mount IDs agree.
Existing canonical-path and directory-identity checks still apply. A bind mount of
the repository root or a nested repository directory has a different mount ID and
is refused before staging. Missing or malformed mount ID data also refuses export.

This is a conservative Linux rule: the destination parent must be outside the
repository **and on the same mount as its root**. An otherwise safe destination on
another mount is refused. A later separately verified design may allow such paths
without weakening containment. No extra crate or filesystem permission is added.

## Consequences

The rule covers a static bind-mounted alias and checked mount changes while a
writer retains its descriptors. It does not prove immunity to every concurrent
same-user mount or rename between checks and publication. The native UI still uses
its own isolated report helper and schema; this correction does not provide exact
CLI Show-command equivalence or complete Task 36.3.2.2.

## Verification

Bounded namespace tests bind the repository root and a nested directory to an
outside alias and exercise both default and explicit-overwrite requests. They
assert refusal before a report leaf is created in the source. The tests are
separate from installed-desktop, live-provider and human qualification.
