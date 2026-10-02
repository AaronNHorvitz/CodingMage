# ADR 0086: Prepare Setup Export Intents on the Worker

- **Status:** Accepted for local implementation; bounded independent exact-commit review passed
- **Date:** 2026-10-02
- **Decision owners:** CodingMage implementation

## Context

The detached Setup export survives a window close by recording a private
intent before launching the public `setup-export-copy` command. Recovery reads,
destination verification and notice cleanup already run on the bounded native
worker, but creation, private file write and synchronization of a new intent
still ran on the render thread. A slow private filesystem could stall the
window during the initial export action.

## Decision

Queue intent preparation on the existing bounded worker with the selected
configuration, diagnosed repository, source, destination and frozen public
command arguments. The worker writes the bounded private intent and returns a
closed response containing its request identity and byte digest. The interface
requires its current selection and exact pending request to match every
returned field before it queues the detached helper. A failed, malformed or
stale response cannot launch the export or claim success; a persisted private
intent remains available for recovery and inspection. The public coordinator
export command and its authority remain unchanged.

This moves the filesystem work off the render thread without granting the
interface execution authority. Private recovery actions still have no public
coordinator command equivalent. The complete section-five state, depth and
Show-command catalogue remains open under Task 36.3.2.2.

## Verification

See `docs/evidence/sprint-36-setup-export-worker-preparation.md` for exact
local checks, failures retained and review status. No new dependency, model,
credential surface or licence decision is introduced. Desktop, human,
live-provider and release gates remain open.

Independent read-only review of exact commit
`761adec6c02a035686e6bd3ff6d6b29c9d1d21e6` returned PASS with no
findings after its own native UI 210/210, focused Setup 20/20, worker unit,
strict Clippy and static checks. It is a bounded engineering review only.
