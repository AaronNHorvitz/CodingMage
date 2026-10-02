# ADR 0082: Reconcile Guided Configuration Through the Worker

- **Status:** Accepted for local implementation; bounded independent review passed at `ab034991da2324e282e75f93e146f8f4d29f25bc`
- **Date:** 2026-10-02
- **Decision owners:** CodingMage implementation

## Context

Decision 0076 made a guided configuration write survive a closed window, but
checking its private terminal record, reading its destination and reopening the
project still happened during native rendering. A slow or malformed file could
stall the window. The direct project parser also bypassed the public
`project-open` snapshot used by ordinary repository selection.

## Decision

Recovery submits the exact private intent path, SHA-256 and request identity to
the existing bounded worker. The worker rejects a changed intent and missing or
invalid terminal result, verifies the successful public write receipt against
the named destination, and invokes the read-only `project-open` coordinator
command with a deadline. Its versioned snapshot must name the requested file,
carry the exact published configuration digest and decode to the submitted
configuration. The worker rechecks the receipt and named bytes after the public
read. A stale selection generation or request identity cannot adopt the
result. No recovery check reruns the write or starts a campaign.

Inspection leaves the private terminal record intact until the UI accepts the
matching result. Only then does a second worker request check the successful
receipt and named bytes again and clear the exact private notice. A failed
cleanup retains the notice and withdraws the provisional project selection.
The explicit “I inspected the destination” action also clears through the
worker, without claiming the write succeeded. Failure to queue a new write
retains its intent for inspection. Private candidates remain outside arguments,
output, logs and command previews.

This extends the native worker and decoder with no new dependency or authority.
It does not complete the section 5 state and depth catalogue, desktop trials or
the qualified-human and live-provider gates.

## Verification

The disposable Setup target exercises real guided publication, failed write,
window close and reopen, missing task source, destination replacement,
selection change and explicit notice clearing. A separate real-process
decoder test refuses a different or malformed published digest. Exact results
and limitations are recorded in
`docs/evidence/sprint-36-guided-configuration-recovery.md`.
