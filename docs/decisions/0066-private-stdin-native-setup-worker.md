# ADR 0066: Route Authorization Setup Through the Bounded Native Worker

- **Status:** Accepted for local implementation; independent review pending
- **Date:** 2026-10-01
- **Decision owners:** CodingMage implementation

## Context

The public `setup-write-authorization` command has a bounded independent
review PASS at `46aeae8da288c46ba3e76a38c59eb4bb745b129b`, but the native
Setup screen still called its in-process file writer on the render thread.
That path could not meet Decision 0016 or the binding native UI specification.
The command accepts its operator-authored record only on private stdin and
returns a content-minimized digest receipt.

## Decision

Add a `PrivateCommand` request to the existing eight-slot backend queue. Its
stdin payload is capped at 1 MiB before queueing and has a redacted debug
representation. The existing sibling executable client pipes it on a separate
thread while supervising the process with the same deadline, cancellation,
bounded stdout/stderr and stable error mapping as read-only commands. It is
never inserted into argv or environment.

The Setup screen sends the exact observed repository ID, configured path,
destination and explicit overwrite choice as command arguments. It shows that
argument vector beside the action and explains the omitted private stdin.
Duplicate in-flight writes are refused. The response must match the selection
generation, binding, request identity, schema version, repository ID, byte
count and SHA-256 digest before the record is selected for preflight. A changed
selection cancels the request and discards a late response. A late response
cannot clear a newer pending write unless its request identity also matches.
Any failed or uncertain outcome tells the owner to inspect the destination
before retrying; the interface never retries a write automatically.

This moves only the authorization action. Configuration, campaign and export
still require public coordinator commands and worker migration. The in-process
Setup writers remain for those open paths until replaced. The worker does not
claim the command's authority for itself, and cancellation cannot prove that
an already published effect did not occur.

## Independent finding and correction

The bounded exact-commit review of `99017d8b1fb6408659c6be2210da6656229ef2da`
returned PASS with a Medium finding: while a write was pending, the displayed
command could be recomputed from edited fields rather than the queued
arguments. The native state now retains the submitted argument vector, shows
that vector until the request resolves or is cancelled, and disables the
record path, text and overwrite widgets during that interval. A display-less
regression changes all three values after submission and checks the displayed
command and actual destination against the submitted values. The correction
requires its own exact-commit re-review.

## Verification

The bounded worker test sends exact private bytes through a disposable
executable, checks the debug redaction and refuses an oversized payload. The
real Setup test exercises both the repository-contained refusal and a
successful external record through the public coordinator command, with the
screen waiting for the matching result. Exact commands and cumulative results
are recorded in the Setup command evidence. Independent review is requested
only for the verified commit. Human, live-provider and release gates remain
open.
