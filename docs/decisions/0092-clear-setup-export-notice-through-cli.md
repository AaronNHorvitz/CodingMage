# ADR 0092: Clear a Bound Setup Export Notice Through the Public CLI

- **Status:** Accepted for local implementation; exact-commit review pending
- **Date:** 2026-10-02
- **Decision owners:** CodingMage implementation

## Context

Decision 0091 made inspection of a prior detached Setup export a public,
read-only command. The separate manual notice-clear button still removed the
private intent and terminal record inside the native worker. That operation
changes state, so the native UI specification's public command boundary and
Show-command rule apply. The detached export helper already holds an exclusive
lock throughout its public copy command and terminal write.

## Decision

Add `codingmage setup-export-clear` with an absolute intent path, exact intent
digest, request ID, configuration path, diagnosed repository ID and explicit
`--verified true|false`. `true` requires the successful public recovery check
against the terminal record and current destination bytes; `false` means the
operator explicitly inspected the destination and makes no success claim.
The command takes the helper's nonblocking lock, refuses a running helper,
validates the configuration-derived private directory, its owner and access
mode, and permits older owner-only-writable state ancestors while requiring
a private project leaf. It uses a held directory descriptor and no-follow
reads for the named intent and result. It removes only those two private
notice files in result-
then-intent order and syncs the directory. It never removes or changes the
exported destination, campaign state, source repository or authorization.

The native button and automatic verified cleanup submit the exact command
shown in their Setup context through the bounded worker. The UI accepts only
a closed, matching clear receipt before dismissing its notice. Malformed,
foreign, stale and failed responses keep recovery visible. The lock and
receipt are an application boundary for this private state, not an authority
grant or proof of an installed desktop or a human inspection.

This uses the existing CLI, native worker and workspace-pinned dependencies;
no dependency version, licence, provider or runtime authority changes.
Guided configuration recovery, the complete section-five state catalogue
and contextual depth remain open under Task 36.3.2.2.

## Verification

See `docs/evidence/sprint-36-setup-export-clear-command.md`. Independent
exact-commit review remains required for this increment.
