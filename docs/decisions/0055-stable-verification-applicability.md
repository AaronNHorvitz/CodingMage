# ADR 0055: Stable Applicability for the Native Failure Panel

- **Status:** Accepted for the inventory correction
- **Date:** 2026-10-01
- **Decision owners:** CodingMage implementation

## Context

The public verification inventory uses a short source neighborhood to suggest
applicable test categories. The Work plan catalogue edits moved unrelated
text near `failure_box`. As a result, its previously indexed boundary category
and candidate test mappings disappeared although the failure panel itself did
not change. The independent cumulative review of the Work plan catalogue
identified that drift as a medium finding. The original generated inventory
and review report are retained in their exact commits and private review record.

## Decision

Declare stable applicability for the public native failure panel by its
repository-relative path, declaration kind and name. Its positive, negative,
boundary and repeatability categories remain in the work queue regardless of
neighboring UI text. The boundary category covers minimum-window and long-text
presentation. Keep the existing heuristic for other entries in this bounded
repair; a broader category model requires a separate inventory migration and
its own review. Test that moving unrelated neighboring words cannot change
this entry, then regenerate the source-consistent inventory.

## Consequences

The declaration and its runtime behavior do not change. Category mappings
remain candidate test references, not proof that a given test exercises the
panel. The correction restores the ancestor's applicability and mapping on
this entry and preserves the source-bound inventory's explicit gaps. It does
not renew CM-R01.6 evidence, certify accessibility or close Task 36.3.2.4.

## Verification

See [the Work plan catalogue evidence](../evidence/sprint-36-message-catalogue.md).
