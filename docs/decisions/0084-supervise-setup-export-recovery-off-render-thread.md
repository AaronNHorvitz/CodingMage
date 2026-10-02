# ADR 0084: Supervise Setup Export Recovery Off the Render Thread

- **Status:** Accepted for local implementation; bounded independent review pending
- **Date:** 2026-10-02
- **Decision owners:** CodingMage implementation

## Context

The Setup export command already runs in a detached helper and retains a
private intent and terminal record after a native window closes. Reopening a
configuration still loaded that private record on the render thread. Checking
the prior outcome, reading the exported destination to verify its digest and
clearing the notice also ran during UI updates. A slow or hostile file could
delay a frame. The old completion path checked a receipt on the render thread
before clearing it.

## Decision

The existing bounded worker now loads an export intent after project selection,
reconciles its exact private terminal record with the named destination, and
clears a notice. The load response is bound to the selected configuration,
generation and request ID; reconciliation and cleanup additionally require
the observed repository identity and exact export ID. A successful
helper response only starts a fresh recovery check. A successful recovery
response only requests a separate verified cleanup; success is shown after
that cleanup succeeds.

The worker checks the private intent digest and request identity on both
recovery and clear. It validates a closed public receipt and reads the named
destination through a held, no-follow, bounded descriptor, then rechecks the
name's file identity. Verified cleanup repeats receipt and destination
validation. An explicit inspected clear may remove the notice without
claiming the export succeeded; the intent lock still refuses clearing a
running helper. A missing or malformed result remains unconfirmed.

The export command, detached helper, coordinator authority and public receipt
are unchanged. Creating the private intent for a new export is a separate
remaining render-thread operation; this decision addresses recovery only.
Task 36.3.2.2 remains open for the full screen-state and in-context command
catalogue. No new dependency, runtime role, credential or provider authority
is introduced.

## Verification

The exact focused, cumulative and independent-review disposition is recorded
in `docs/evidence/sprint-36-setup-export-recovery.md`.
