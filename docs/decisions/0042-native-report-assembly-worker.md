# ADR 0042: Assemble native reports outside rendering

- **Status:** Accepted
- **Date:** 2026-09-30
- **Decision owners:** CodingMage implementation

## Context

The Reports screen assembled a complete outcome document on every repaint and serialized its
inline JSON preview on every frame while the preview was open. Its guarded export writer ran
outside the render path, but these preparation steps could still delay input and scrolling.
Report preparation must remain bound to the campaign and observations the owner is viewing.

## Decision

The interface captures an owned snapshot of its current coordinator observations and sends it
to one dedicated CPU-only report worker. That worker assembles the document and serializes a
preview capped at 128 KiB. The full document remains available to the existing isolated export
writer. The report worker has a single queued request, performs no filesystem or coordinator
operation, and wakes the interface when its result is ready. It is separate from the worker that
runs campaign controls, so report preparation cannot queue ahead of those commands.

The interface keys each result to the selection generation, source-response revision, source
observation and freshness states, admission, last terminal invocation and the path-inclusion
choice. It discards an obsolete result and assembles again for the current key. Until a matching
result exists, the screen shows preparation and refuses export. A changed source cannot reuse an
old preview or report. Selection changes retain the one in-flight work identity until its result
arrives, preventing a succession of queued snapshots. Shutdown closes the request queue and
joins the finite CPU-only worker; the response channel cannot block that join.

## Limits

Capturing owned observations still clones bounded source projections on the UI thread. The
preview is deliberately incomplete when it reaches the limit and is labelled as truncated; the
export document retains all report fields. A synchronous assembly API remains for tests and
diagnostics, while the production rendering and export paths use the worker result. This change
does not create a public `codingmage` report-export command or close the wider native screen,
accessibility, performance, human, live-provider, independent whole-product or release gates.

## Verification

See [the report assembly evidence](../evidence/sprint-36-report-assembly.md).
