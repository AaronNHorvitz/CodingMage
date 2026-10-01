# ADR 0051: Externalize Work Plan Orientation and Observation Copy

- **Status:** Accepted for an incremental native catalogue slice
- **Date:** 2026-10-01
- **Decision owners:** CodingMage implementation

## Context

The native Work plan still embedded its first-run, source, filter, coordinator
observation and final-report recovery text in Rust. The bundled catalogue
already validates exact English keys and named fields for other screens.

## Decision

Move those Work plan messages into the existing version-one English catalogue.
Require the source path, shown and total counts, freshness, age, head freshness
and recovery action fields exactly once through its existing template checks.
The UI inserts runtime values as inert text. The shipped interface continues to
use English; there is no language or expertise mode. Exercise expanded and
right-aligned synthetic copy at the minimum window and double scale.

## Consequences

Coordinator command arguments and campaign authority do not change. The
Work plan's row labels, source detail, task evidence, locale-aware numbers and
full bidirectional keyboard navigation remain separate open work under Task
36.3.2.4. A synthetic right-aligned preview is not installed Orca or human
qualification.

## Verification

See [the catalogue evidence](../evidence/sprint-36-message-catalogue.md).
