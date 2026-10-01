# ADR 0061: Verify the Private Stage Before Use

- **Status:** Accepted for local correction; independent re-review pending
- **Date:** 2026-10-01
- **Decision owners:** CodingMage implementation

## Context

Decision 0060 creates a mode-0700 staging directory below a checked destination
parent, then opens it by name. The exact-commit independent review of
`e5e83af254470d7a620f7ed7497a780dc2aef86c` found that a writer with
parent-directory access can replace that name between `mkdirat` and `open`.
The code verified only that the opened descriptor matched the current name.
If the replacement directory belongs to another principal, that principal can
replace the candidate after validation and before exchange. The writer then
reports uncertainty, but the unvalidated bytes can be present at the public
destination. The private review report and earlier local receipts remain
unchanged.

## Decision

Before any candidate is created, check the opened directory descriptor's
metadata: it must be a directory owned by the process's effective user ID, with
owner read/write/search and no group or other permission bits. Also require its
device and inode to match the current staging name. Refuse a failed or changed
open as a destination conflict. Do not remove the substituted name or use the
opened directory on mismatch. Continue all candidate operations relative to
the verified, held descriptor. The existing
exchange, displaced-entry checks and uncertain recovery behavior remain.

The pinned `nix` dependency enables its `user` feature for safe `geteuid`;
there is no new package or licence. A writer running as the same user can still
mutate a directory it owns. That actor remains outside the private-stage
isolation promise and is not treated as independent acceptance.

## Consequences

A different principal with write access only to the destination parent can
rename the just-created stage, but cannot make its replacement pass both the
effective-owner and private-mode checks. On a conflict, the old destination
remains unchanged. An orphaned stage may remain under the name to which the
other writer moved it; the writer does not guess that name or delete a
substituted directory. The rest of the native Setup command, asynchronous and
Show-command requirements remain open.

## Verification

See [the Setup writer evidence](../evidence/sprint-36-setup-writer-hardening.md)
for the deterministic substitution and metadata checks, applicable cumulative
gates and independent review disposition.
