# ADR 0074: Publish Guided Configuration Through the Coordinator

- **Status:** Accepted for local command implementation; native client and independent review pending
- **Date:** 2026-10-01
- **Decision owners:** CodingMage implementation

## Context

The native guided configuration form still creates the scratch and state roots
and writes the configuration inside the interface process. The native
specification requires repository and filesystem effects to pass through the
public coordinator command boundary. Bootstrap cannot require an existing
configuration or its diagnosis.

## Decision

Add `codingmage setup-write-config --repo ... --output ...` with at most 1 MiB
of private TOML on stdin. The exact selected repository path is checked
against the proposed configuration; the command retains repository identity
and Git HEAD through final publication. The candidate must pass the existing
configuration loader before guarded outside-repository publication. Existing
scratch and state roots are checked as trusted external directories. Missing
roots may be created only as private direct children of the configuration's
checked parent, which covers the guided form's default layout. The command
checks their physical identities before and after publication. It defaults to
no overwrite. Explicit overwrite requires an existing valid configuration
bound to the same repository, with its held bytes rechecked immediately before
publication. The success receipt contains only repository identity, observed
head, byte count and digest. A final held, no-follow read checks the named
destination bytes and authority once more before returning that receipt.

An error after root creation may leave an empty private directory, which the
operator must inspect. A post-publication authority failure remains an
uncertain write, never a success receipt. There is no automatic retry. The
native form remains on its old in-process path until a separate bound worker,
window-lifetime recovery and exact Show-command are implemented and reviewed.
This CLI increment grants no new campaign authority or provider capability.

## Verification

Real-process disposable tests exercise first publication with private root
creation, exact bytes and receipt, no-overwrite refusal, explicit replacement,
malformed and oversized input, foreign repository, inside-repository output,
foreign-configuration overwrite, nonlocal missing root and linked root or
output. Existing guarded-writer tests retain
the publication race contract. Exact command-source checks and limitations are
recorded in the Setup configuration evidence. Native, human, live-provider
and release qualification remain open.
