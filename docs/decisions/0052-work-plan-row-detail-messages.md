# ADR 0052: Catalogue Work Plan Rows and Parsed Detail

- **Status:** Accepted for an incremental native catalogue slice
- **Date:** 2026-10-01
- **Decision owners:** CodingMage implementation

## Context

The Work plan's first-run and observation copy uses the validated English
catalogue, while its loaded rows, filters and parsed item detail still embed
English text in Rust. These paths display source claims and coordinator
outcomes together, so a missing translated field could obscure their distinct
identities.

## Decision

Use the existing catalogue for filter names, row headers, checkbox guidance,
outcome badge labels and parsed item detail. Declare each applicable runtime
identifier, title, state, readiness, line and digest field exactly once in its
template. Keep source text inert and bounded through the existing content
renderer. Exercise a loaded plan at the minimum window and double scale with
expanded and right-aligned synthetic catalogue text.

## Consequences

The task parser, source checkbox and coordinator authority do not change.
The public `workplan` crate's English label helpers remain for compatibility;
the native Work plan uses catalogue values. Coordinator-provided detail,
source excerpts, run evidence, locale formats and full bidirectional
navigation remain open under Task 36.3.2.4. The synthetic preview does not
qualify installed accessibility or human use.

## Verification

See [the catalogue evidence](../evidence/sprint-36-message-catalogue.md).
