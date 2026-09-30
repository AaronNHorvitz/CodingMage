# ADR 0041: Isolate native report writing from campaign controls

- **Status:** Accepted
- **Date:** 2026-09-30
- **Decision owners:** CodingMage implementation

## Context

Decision 0040 moved report serialization and filesystem writing off the render thread,
but its one worker queue also executes coordinator controls. An independent exact-commit
review found that a filesystem call stalled indefinitely could block Stop/Cancel behind
that export and keep window shutdown waiting on the worker join. A cancellation flag
cannot interrupt an uninterruptible filesystem syscall.

## Decision

The existing bounded worker dispatches one report export to a separate supervisor and
immediately resumes coordinator requests. The supervisor starts a one-shot process using
the installed `codingmage-ui` executable's private helper mode. It transfers the immutable
report, destination, selected repository and overwrite choice through a bounded stdin
pipe; none of those values appear in the process arguments or environment. The helper
uses the unchanged directory-handle writer, including its repository identity, privacy
and overwrite checks, and returns a bounded result through stdout. Only one helper may
be live or awaiting reap at a time. A selection change or window shutdown signals
cancellation; a 30-second deadline terminates a stalled helper. Reaping can continue
without holding the UI or coordinator worker if the kernel delays termination.

The UI accepts only the original request identity and selection binding. A timeout is
shown as an uncertain destination requiring inspection before retry. The internal helper
is a containment boundary for one explicitly requested local export, not a new public
campaign command or source of authority. The desktop executable must be built and
installed beside the coordinator executable for integration tests and operation.

## Limits

An uninterruptible filesystem call can delay process termination even after it is
signaled. The UI refuses another report writer until the prior child is reaped. If the
child was terminated after creating its private temporary file, a
`.codingmage-report-*.candidate` file may remain in the chosen parent directory; the
owner must inspect the destination and any candidate before retrying. A completed write
may race a late cancellation; the report view never labels a stale response as current.
The concurrent same-user rename limits in Decision 0038 remain. The report is still
assembled from UI observations and lacks an exact sibling `codingmage` export command,
so the public-command rule and Task 36.3.2.2 remain open. This change does not qualify
desktop, human, live-provider, security, independent whole-product or release gates.

## Verification

See [the isolated export evidence](../evidence/sprint-36-report-isolation.md).
