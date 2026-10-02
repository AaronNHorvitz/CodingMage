# ADR 0073: Keep a Started Setup Export Independent of the Window

- **Status:** Accepted for local implementation; independent re-review pending
- **Date:** 2026-10-01
- **Decision owners:** CodingMage implementation

## Context

The first native `setup-export-copy` client used the general worker command.
That worker cancels a child when the selected campaign or project changes and
when the window closes. A cancellation after publication but before delivery of
the receipt leaves the destination changed without a dependable native result.
An independent read-only review of the exact first commit found this lifecycle
gap. The public coordinator's guarded writer and source checks remain the
authority for the export itself.

## Decision

Before dispatch, Setup writes a create-only private intent containing a random
request ID, the observed repository identity, exact source and destination,
and the command arguments already shown to the owner. A separate native helper
process starts the unchanged public coordinator command with a fixed deadline.
The helper checks that the private intent still has the submitted SHA-256, holds
an exclusive per-project intent lock during execution, and records a bounded
terminal result in private state. The UI worker waits in a detached supervisor;
selection changes and window close discard stale presentation without signaling
the started helper. Reopening the exact project checks the terminal result,
public receipt, repository identity and held destination bytes before reporting
success. A missing or invalid result remains an uncertainty, never a pass.

The owner may clear an unresolved notice only after inspecting its destination.
Clearing requires the private lock, so an active helper cannot be cleared or
overlapped. If an earlier queued helper starts after clearing, its intent digest
no longer matches and it refuses to run the coordinator. A submitted intent is
synced before dispatch; the terminal record is synced before helper exit. The
result is bounded private UI state, not coordinator authority or a new mode.

## Verification

Disposable native tests hold the public exporter before and after publication,
then close the window or reselect a campaign. They check that the command
finishes, stale presentation is rejected and reopening verifies the retained
result. The active-helper lock refuses premature clearing. A unit test changes
the persisted intent after submission and requires a digest mismatch. The
separate short-lived launcher fixture proves the helper can record its result
after its parent process exits on Linux. The
ordinary public CLI export tests remain the source and publication checks.
Installed desktop, human, live-provider and release acceptance remain open.
