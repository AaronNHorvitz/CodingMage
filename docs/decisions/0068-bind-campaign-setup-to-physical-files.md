# ADR 0068: Bind Campaign Setup Inputs to Physical Files

- **Status:** Accepted for local implementation; independent re-review pending
- **Date:** 2026-10-01
- **Decision owners:** CodingMage implementation

## Context

The first independent review of Decision 0067 found that two names for the
same file can evade a parent-and-leaf path comparison. A hard link to the
configuration was accepted as the external authorization record when the
candidate supplied the matching digest. This violates the requirement that
the configuration cannot authorize itself. The previous review report remains
unaltered and applies to its exact commit only.

## Decision

Open the configuration and authorization record with no-follow descriptors
and retain their device and inode identities through guarded publication.
Reject either input when its named leaf no longer refers to the held regular
file. Read the bounded authorization bytes from that held descriptor, and
compare the two physical identities before accepting the candidate. Also
reject a campaign destination that currently names either protected inode,
including through a hard link. Repeat the identity and destination checks at
the guarded publication boundary. Existing normalized path comparisons still
catch direct and parent-alias collisions. The report writer continues to own
atomic publication, overwrite and post-publication uncertainty semantics.

This protects the document boundary against stable hard-link aliases and
detected replacement between observation and publication. It does not grant
the record authority on its own or guarantee stability after the command
returns. No dependency or licence changes are needed; the CLI already uses
the pinned `nix` dependency for no-follow file operations.

## Verification

Disposable real-process regressions submit a hard-linked configuration as the
authorization record with a matching digest, request replacement through
hard-linked configuration and record destinations, and request replacement
through a parent-directory alias. Each must refuse publication and preserve
the protected bytes. Exact local results and the independent re-review status
are recorded in the Setup command evidence.
