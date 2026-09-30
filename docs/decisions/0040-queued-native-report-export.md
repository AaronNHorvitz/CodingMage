# ADR 0040: Queue native report export away from rendering

- **Status:** Accepted
- **Date:** 2026-09-30
- **Decision owners:** CodingMage implementation lane

## Context

The Reports screen formats and writes an outcome document after an explicit owner request.
Its directory-handle writer checks destination and repository identity, privacy and
overwrite consent, but both JSON formatting and file I/O ran on the native render thread.
An external filesystem can delay either operation. The native specification requires
long work to run off that thread.

## Decision

Capture the report, destination, repository and overwrite choice when Export is pressed.
Queue that immutable snapshot on the existing bounded UI worker. The worker invokes the
unchanged directory-handle writer and wakes the interface with a result carrying the
selection generation, repository/campaign binding and unique request identity. Refuse a
second export while one is pending. A response from a different request or a selection
that has since changed cannot update the current Reports screen. Show pending and exact
success or refusal only after a matching result arrives.

This is a presentation-thread correction. It does not create a second report authority,
change coordinator records or claim that the report has been independently verified.
The export was explicitly requested at queue time; a started file write may finish even
if the owner later changes the selected view.

## Limits

The report is still assembled from UI observations. Export is still a local UI worker
operation with no exact sibling `codingmage` command equivalent, so Decision 0016 and
the native specification's public-command rule remain open under Task 36.3.2.2.
Report assembly and on-screen JSON preview still run during rendering. Concurrent
malicious same-user directory rename and overwrite-leaf races have the limits in
Decision 0038. Human, live-provider and release gates remain open.

## Verification

See [the report worker evidence](../evidence/sprint-36-report-worker.md).
