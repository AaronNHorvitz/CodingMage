# ADR 0004: Append-Only Journal and Atomic Snapshot

- **Status:** Accepted
- **Date:** 2026-08-19
- **Decision owners:** Repository owner
- **Supersedes:** None
- **Superseded by:** None

## Context

Long-running workflows must survive crashes, interrupted model sessions, quotas, and host restarts
without replaying completed side effects or trusting an incomplete current-state file.

## Decision

Persist every accepted state transition as a versioned event in an append-only JSON Lines journal.
Derive a bounded current-state snapshot and replace it atomically only after the corresponding
journal record is durable. On recovery, validate the event chain and rebuild the snapshot when it
is missing or stale. Unknown schema versions, gaps, forks, or invalid transitions fail closed.

## Alternatives Considered

- Snapshot-only storage cannot distinguish a torn write from a valid prior state.
- A database adds migration and operational complexity before the single-host workflow needs it.
- Reconstructing state from Git and provider transcripts would treat external side effects and
  untrusted prose as canonical authority.

## Consequences

The journal requires retention, compaction, locking, redaction, and corruption tests. It provides
an auditable recovery boundary and keeps runtime state independent of target repositories.

## Verification

- Crash-point tests cover append, synchronization, rename, and directory synchronization stages.
- Replay is deterministic and rejects invalid chains without mutating the target.
- Evidence proves duplicate execution is not inferred from a stale snapshot.
