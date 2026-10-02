# ADR 0085: Bind Setup Export Cleanup to the Observed Repository

- **Status:** Accepted for local implementation; independent re-review pending
- **Date:** 2026-10-02
- **Decision owners:** CodingMage implementation

## Context

The first independent review of Decision 0084 found that manual notice cleanup
could delete a previous export's private recovery record after the same
configuration pathname was reopened for a different repository. Recovery
inspection already refused that mismatch, but the cleanup request carried
only the private intent identity and request ID. The retained terminal result
belongs to the original repository observation and must remain available.

## Decision

Both verified and manually requested cleanup require the current selected
configuration and diagnosed repository identity to equal the retained export
intent. The native control is disabled when either observation is missing or
different, and a direct cleanup request also refuses the mismatch. The bound
worker receives the observed configuration and repository identity, then
rechecks both against the exact intent digest and request ID before removing
any private record. A refusal leaves the intent and terminal result intact
and directs the owner to inspect the destination from the original repository.

No coordinator export command, helper lifecycle, or runtime authority changes.
The private recovery controls still lack a public command equivalent, so
Task 36.3.2.2 remains open. This correction does not assert real desktop,
human, live-provider, or release qualification.

## Verification

See `docs/evidence/sprint-36-setup-export-identity-correction.md` for exact
local checks and independent re-review status.
