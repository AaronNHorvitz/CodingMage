# ADR 0037: Resolve report export parents before writing

- **Status:** Accepted
- **Date:** 2026-09-30
- **Decision owners:** CodingMage implementation

## Context

The native Reports screen permits an explicit JSON export outside the selected target
repository. Its previous check compared the destination and repository as written path
strings. A parent-directory component or a symbolic link in the destination's parent could
resolve into the repository even when the strings had different prefixes.

## Decision

Require an absolute export path without parent-directory components and an existing,
readable parent directory. Resolve both that parent and the repository before checking
containment. Refuse a parent that resolves to the repository or one of its descendants,
including a parent reached through a symbolic link. Do this before any export parent is
created or report bytes are written. Keep the existing destination symlink and overwrite
checks in the validated writer.

This is a local export safeguard over already observed coordinator records. It adds no
coordinator command, agent authority, provider, dependency or licence. The UI's existing
export action still performs file I/O on the presentation thread and still lacks a
`codingmage` command equivalent; those broader section-2/section-7 specification gaps
remain open under Task 36.3.2.2.

## Limits

The path check and write are separate filesystem operations. This increment checks
static destination resolution; it does not qualify a race against another local process
renaming or replacing parent directories during export. Such a race requires a directory
handle-based writer and separate verification before any stronger claim.
