# ADR 0064: Publish Setup Authorization Through the Public CLI

- **Status:** Accepted for local implementation; independent review pending
- **Date:** 2026-10-01
- **Decision owners:** CodingMage implementation

## Context

Guided Setup still writes configuration, campaign, authorization and export
files inside the native UI process on its rendering thread. That violates the
public sibling-command and nonblocking rules in Decision 0016 and the native
UI specification. The existing CLI report writer now has a reviewed
repository-bound private publication stage. The authorization record is the
first Setup write to move across that boundary because its text must remain
private and its digest is later bound into campaign authority.

## Decision

Add `codingmage setup-write-authorization --config <file> --repository-id
<observed-id> --output <file> [--overwrite true|false]`. The operator supplies
the exact UTF-8 record on stdin, up to 1 MiB; it is never an argument. The
command validates the selected configuration, authorizes its repository through
the existing repository contract, requires the exact identity previously
observed by `doctor`, and checks the destination before reading the record. It
revalidates the held repository and Git HEAD at the guarded publication
boundary described in Decision 0065. It refuses blank, malformed,
NUL-containing or oversized input. The reviewed CLI report writer publishes
the exact bytes outside the repository with no silent
overwrite. A successful machine-readable receipt contains only the schema
version, repository identity, byte count and SHA-256 digest, not the text or
path. Errors remain stable content-free codes on stderr.

This adds a direct use of the existing workspace-pinned `sha2` 0.11.0 crate to
the CLI manifest. Its recorded licence is MIT OR Apache-2.0 (Decision 0015);
the lockfile gains only the CLI's direct edge, with no new package, version or
licence policy.

## Consequences

The command makes one public CLI equivalent available for Setup. The native UI
still needs a bounded off-render worker, private stdin delivery and an exact
Show-command disclosure that explains stdin without showing the record.
Configuration, campaign and document-export writes also remain in the UI
process until separate public commands and migration exist. None of this
changes campaign admission or grants authority by merely opening Setup.

## Verification

See [the Setup command evidence](../evidence/sprint-36-setup-command-boundary.md).
Installed desktop, real assistive tool, human trials, live provider and release
qualification remain separate.

## Independent finding and correction

The first exact-commit independent review of `d77a67414af09b5669c1483c21e442d5d8fe0454`
returned `FINDINGS`: a changed Git HEAD during candidate staging could still
produce a success receipt. The report is retained in private review state.
[Decision 0065](0065-revalidate-setup-authority-at-publication.md) specifies the
final-boundary correction and its uncertain-write behavior. Its own exact-commit
re-review remains pending.
