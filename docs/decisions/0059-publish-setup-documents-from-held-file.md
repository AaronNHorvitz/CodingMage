# ADR 0059: Publish Setup Documents from the Held File

- **Status:** Candidate binding retained; overwrite protocol superseded by Decision 0060
- **Date:** 2026-10-01
- **Decision owners:** CodingMage implementation

## Context

The exact-commit independent review of Decision 0058 found a high-severity race.
The existing loader validated a named candidate, but publication used that name.
Replacing the candidate after validation could publish rejected bytes. The writer
returned an error after its post-publication identity check yet left those bytes
at the requested destination. The retained review report is identified in the
Setup writer evidence; its verdict remains FINDINGS.

## Decision

Keep the existing loader check, then compare the candidate directory entry with
the retained file descriptor and compare the descriptor's bytes with the bytes
submitted to the writer. Publish by linking that descriptor directly to the
destination, never by linking or renaming the mutable candidate name. A removed
candidate cannot cause replacement bytes to be published. Clean up a candidate
or newly published entry only while it still has the writer's inode.

For an explicitly requested overwrite, first retain a hard-link backup of the
existing regular file, remove the old leaf, and link the validated descriptor.
Keep the backup until final identity and repository-parent checks pass. Attempt
to restore it if publication fails; if another entry prevents restoration, retain
the backup and report an I/O error for manual reconciliation. This overwrite
sequence is not atomic against concurrent writers and is not a transaction.

## Consequences

The candidate replacement found by the reviewer cannot publish its replacement
bytes. A process with the same user identity can still change files it owns;
this writer does not provide hostile same-user process isolation. Explicit
overwrite may briefly leave the destination absent, and a concurrent leaf
change can require manual reconciliation. These limits are visible failures,
not successful publication claims.

The independent re-review of this exact implementation passed the earlier
candidate-publication finding but found that the check immediately before
removing an overwrite leaf could race another directory writer. In that
interleaving, this implementation could report success after deleting the
writer's replacement. The final sentence above was therefore too strong for
this revision. [Decision 0060](0060-exchange-overwrite-with-private-stage.md)
replaces the overwrite protocol; the original review finding remains retained.

The native UI still performs guided Setup writes in its own process on the
render thread. Public sibling commands, bounded asynchronous work and exact
Show-command disclosures remain required by Decision 0016 and the native UI
specification. No independent, human, live-provider or release gate is closed
by this local correction.

## Verification

See [the Setup writer evidence](../evidence/sprint-36-setup-writer-hardening.md)
for the retained finding, adversarial cases, exact check results and independent
re-review disposition.
