# ADR 0071: Bind Native Campaign Selection to the Write Receipt

- **Status:** Accepted for local implementation; bounded independent re-review PASS at `0fcbb1e43eb7d1d1874b69f19b58c748923b9162`
- **Date:** 2026-10-01
- **Decision owners:** CodingMage implementation

## Context

The native Setup worker validates the public campaign writer's receipt against
the exact submitted candidate. Independent review of the native increment at
`0873af470aaf9cc0f4905da0eb581ddd3c832301` found that the interface then
reopened the destination by name and selected any valid campaign with the same
identity. A concurrent replacement of that external file could therefore make
Setup report success for authority bytes the writer receipt did not confirm.

## Decision

On a matching write receipt, the interface opens the destination once without
following a final symlink, refuses nonregular or unexpectedly sized files,
reads at most the receipt's bounded length plus one byte, and compares the
SHA-256 of that exact buffer to the receipt. It parses and verifies the same
buffer for selection and applies the opened repository checks. No second
pathname load may intervene between the digest comparison and the selected
authority value. A mismatch leaves no campaign selected and tells the owner
to inspect the destination before retrying. The existing manual campaign
selection remains a separate, unconfirmed path.

This prevents the UI from claiming that the writer confirmed a different
document. It does not freeze a user-selected external path against later
mutation. Coordinator controls continue to make their own fresh authority
checks before effects. Moving the remaining read and Setup configuration and
export actions through the public worker remains open under Task 36.3.2.2.

## Verification

A disposable real-process regression holds the public write receipt, atomically
replaces the destination with a valid same-ID specification whose authority
differs, then releases the response. It requires a receipt mismatch, no
selection and no success. Exact-source checks and re-review disposition are
recorded in the Setup command evidence; this decision alone is not review or
human, live-provider or release acceptance.
