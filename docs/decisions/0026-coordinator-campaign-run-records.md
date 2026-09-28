# Decision 0026: Coordinator Projection of Campaign Run Evidence

- **Status:** Accepted for incremental native UI implementation
- **Date:** 2026-09-28
- **Decision owners:** CodingMage implementation worker under delegated UI scope
- **Supersedes:** The native UI's private run-directory scan
- **Superseded by:** None

## Context

The Changes screen recursively discovered `runs` directories beneath a selected campaign's
private state. The scan collected an unbounded number of directory entries before truncation,
read checkpoint bytes without a size cap, and treated any named run directory as part of the
campaign. The caller's state-directory path was not an authority record. This could display an
unrelated or malformed run as campaign evidence.

## Decision

Add read-only `codingmage campaign-run-records --config ... --campaign ...`. The coordinator
loads the exact campaign authority and durable status. For a serial campaign it derives run IDs
and task bindings from the integrity-valid campaign checkpoint journal. For a parallel campaign
it uses the integrity-valid team snapshot's task, run and lease bindings. The command constructs
private paths internally and accepts no state path or run ID from the UI. It checks the durable
head and update time again after the reads.

The projection carries version 1, campaign and repository identities, the reconciled head and
status update time, at most 500 run records, and a truncation flag. Each record contains the
bound task ID, a validated checkpoint or a specific problem, at most 256 integrity-valid journal
phases, and a truncation or problem indicator. The reader caps a run journal at four MiB, the
campaign journal at sixteen MiB and a checkpoint at 64 KiB. Linked or invalid directories and
files cannot supply evidence. The complete response is capped at four MiB. It carries no private
absolute directory, prompt, source text or provider output. Absence and corruption never become
a review or gate pass.

Each unit runs against a coordinator-owned campaign worktree, whose physical repository ID is
different from the source repository ID in the campaign authority. The campaign checkpoint or
team snapshot binds the run ID and task ID to its private journal path; the verified journal must
keep one internal repository ID across all events. An empty journal is unavailable evidence.

The native UI requests this through its existing bounded sibling-command worker. It rejects
unknown schema fields and a response whose campaign, repository, head or durable update time
differs from its current observations. It clears a prior success on malformed, stale or failed
responses. The UI no longer scans coordinator state directly. The exported UI report advances to
schema 2 and marks whether run evidence was observed and whether its run list was truncated.

## Limits

A changing campaign can cause a transient refused observation; a later status refresh retries.
A journal or response beyond the bounds is unavailable evidence rather than a partial success.
The current backend retains verdict and gate identities but not reviewer finding prose. This
command does not establish independent acceptance, live-provider qualification or delivery.
