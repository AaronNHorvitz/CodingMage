# ADR 0060: Exchange an Overwrite with a Private Stage

- **Status:** Superseded for private-stage admission by Decision 0061; the exchange protocol remains in use
- **Date:** 2026-10-01
- **Decision owners:** CodingMage implementation

## Context

Independent review of the held-descriptor writer passed the earlier candidate
replacement finding. It also found that explicit overwrite checked the old
destination inode and then unlinked its name. A concurrent directory writer
could replace that name between those operations, causing successful deletion
of a different file. The same review found an incorrect count in the generated
inventory comparison; the evidence corrects that count separately.

## Decision

Create the validated candidate in a held, mode-0700 staging directory beneath
the checked destination parent. Keep the exact-byte and retained-descriptor
checks. No-overwrite publication remains a create-only link from the held file.
For an existing regular destination with explicit overwrite, atomically
exchange the staged candidate and destination names with Linux `renameat2`.
Inspect the displaced inode inside the private stage. If it is not the old
inode observed before the exchange, exchange back when the destination is still
our candidate and return a conflict. If restoration cannot be proven, retain
the displaced entry and report its last resolved location, when available, as
an uncertain write. Never unlink a public destination name during overwrite
cleanup.

The private directory prevents another principal with only parent-directory
write access from replacing the displaced entry before it is inspected or
removed. A process acting as the same user can still mutate files and directory
entries it owns; this protocol is not a transaction against that actor.

An independent review of the exact implementation found that the directory
name could be replaced between `mkdirat` and `open`. The held directory might
therefore belong to another principal and be writable by that principal. This
paragraph describes the intended boundary, not a property established by this
decision's original implementation. Decision 0061 adds the missing admission
check before a candidate is created.

## Consequences

A concurrent replacement can be displaced briefly by the atomic exchange, but
the writer returns an error and restores it when the candidate still occupies
the destination. If the destination changes again during restoration, both
entries are retained for reconciliation and the write is explicitly uncertain.
The uncertainty message gives the last resolved path of the retained entry when
available; a simultaneous directory move can make that path stale.
An abnormal filesystem or concurrent parent change can likewise leave a
recoverable file. No false success or destructive leaf unlink is accepted for
the review's deterministic interleaving. The native UI's public command,
asynchronous and Show-command gaps remain open; no external acceptance follows
from this local correction.

## Verification

See [the Setup writer evidence](../evidence/sprint-36-setup-writer-hardening.md)
for the retained independent findings, deterministic replacement tests and
source-bound check results.
