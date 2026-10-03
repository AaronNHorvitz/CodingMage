# ADR 0093: Bind Verified Export Notice Clearing to Held Records

- **Status:** Accepted for local correction; exact-commit re-review pending
- **Date:** 2026-10-02
- **Decision owners:** CodingMage implementation

## Context

Independent review of Decision 0092's exact commit found that verified notice
clearing read a terminal record through a held private directory, then called
the pathname-based, read-only recovery command. A replacement terminal could
therefore be verified in place of the record that clearing was about to remove.
The destination could also change after recovery read it. That made a verified
clear receipt possible for a stale observed destination.

## Decision

Verified clearing parses the exact terminal bytes read from the held private
directory and validates that record's receipt against the named destination.
It remembers device, inode and bytes for intent, terminal and destination;
the named helper lock must remain the held lock. It rechecks these observations
immediately before removing the terminal and again before removing the intent.
Any changed observation refuses the clear while retaining the records that
have not yet been removed. A destination change after terminal removal may
leave an intent without a terminal; the command returns a failure rather than
claiming verified success, and manual inspection remains the recovery path.

The public, read-only recovery command remains a separate observation.
Unrelated same-user filesystem processes are outside the helper-lock protocol;
the final checks narrow their race interval but cannot make a pathname unlink
and an external destination immutable. No new authority, dependency or licence
is introduced.

## Verification

See `docs/evidence/sprint-36-setup-export-clear-race-correction.md` for local
tests and the independent re-review disposition when available.
