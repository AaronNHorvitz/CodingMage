# ADR 0057: Catalogue Native Readiness Copy and Empty State

- **Status:** Accepted for the readiness-panel increment
- **Date:** 2026-10-01
- **Decision owners:** CodingMage implementation

## Context

The Campaign readiness panel still contained English prose in its rendering code,
including the detailed coordinator preflight summary. Its local-check line could
say zero checks passed when no campaign had been selected, which was a count of
unavailable input rather than a readiness result. The existing English catalogue
already validates an exact key set and named template fields.

## Decision

Move the panel's authored labels, progress and stale guidance, and coordinator
summary prose into the same validated catalogue. Keep coordinator supplied
identities, status and counts as literal inserted fields. When repository or
campaign selection is absent, explain that no local readiness result is available
instead of presenting a zero-check result. Give the preflight action a named
Show-command disclosure while retaining the exact sibling-command arguments and
deny-first preview guard. Keep all writes, preflight and admission authority in
their existing components.

## Consequences

The panel continues to ship English only. Synthetic 40% expansion and right
alignment test the missing-selection view; they do not qualify bidirectional
navigation or an installed screen reader. Locale-aware numbers, dates and
durations, other Setup copy, a validated configuration/defaults view and every
remaining command-equivalence gap remain open. No dependency or licence changes.

## Verification

See [the Sprint 36 message-catalogue evidence](../evidence/sprint-36-message-catalogue.md).
