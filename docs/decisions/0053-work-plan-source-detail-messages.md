# ADR 0053: Catalogue Bound Work Plan Source Detail

- **Status:** Accepted for an incremental native catalogue slice
- **Date:** 2026-10-01
- **Decision owners:** CodingMage implementation

## Context

The Work plan's parsed rows and item detail use the validated English catalogue,
but the separate coordinator source-detail request and its result still embed
English in Rust. That result is visible only for a selected item at the fresh,
reconciled campaign head. Its source checkbox and story criteria are source
claims, never verified outcomes.

## Decision

Use the existing catalogue for the request action, command disclosure label,
unavailable/loading/failure guidance, bound-head source-state statement, excerpt
and criteria headings, and truncation notices. Require named head, state and
error-code fields in the relevant templates. Reuse the same action label for
the button and exact-command disclosure. Keep source excerpts and criterion
titles in the existing bounded inert text path. Test an expanded, right-aligned
synthetic result at the minimum window and double scale.

## Consequences

The coordinator command, freshness binding, backend schema and source limits
do not change. Work plan run-evidence copy, other screens' text, locale formats,
full bidirectional behavior and installed assistive-tool verification remain
open. A synthetic preview cannot qualify a real desktop or human workflow.

## Verification

See [the catalogue evidence](../evidence/sprint-36-message-catalogue.md).
