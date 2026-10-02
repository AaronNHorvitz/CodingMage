# ADR 0072: Publish Guided Setup Exports Through the Coordinator

- **Status:** Accepted for local implementation; independent review pending
- **Date:** 2026-10-01
- **Decision owners:** CodingMage implementation

## Context

The native Setup screen copied configuration and campaign documents on its render
thread. The existing public report writer has guarded outside-repository
publication, but the Setup copy did not use that coordinator boundary. It also
had no exact command preview or content-bound response confirmation.

## Decision

Add `codingmage setup-export-copy` for an explicitly named configuration or
campaign source, observed repository ID and external destination. It opens a
bounded regular source without following a final symlink, checks that the source
is the selected configuration or a campaign document bound to that repository
and the exact selected campaign authority SHA-256, then uses the existing
guarded writer. Before and after publication it rechecks repository authority,
configuration, source identity and source bytes. It refuses an output path or
inode that aliases either protected input, defaults
to no overwrite, and returns only repository identity, byte count and digest.

The native action accepts only the opened configuration or currently selected
campaign as its source, then submits that exact command, including the selected
campaign authority digest when applicable, to the existing bounded worker.
It freezes the submitted preview and destination while pending, refuses a
duplicate, discards a stale selection response, and opens the output once
without following a final symlink to compare its exact bytes with the receipt
and rechecks the named output inode before reporting success. A mismatch or
uncertain write asks the owner to inspect the destination. The existing library
copy function remains for its other callers; the native action no longer uses
it. Configuration bootstrap, remaining selection reads and the full screen
state catalogue remain open.

## Verification

Disposable real-process command tests cover configuration and campaign copies,
receipts, no overwrite, explicit replacement, malformed, foreign and
same-repository authority-changed sources, wrong repository identity,
oversized input and protected paths. Native tests exercise the exact preview,
worker result and a destination replacement after the public receipt.
Exact-source cumulative checks and independent disposition
are recorded in the Setup command evidence. These checks do not establish
human, live-provider or release acceptance.
