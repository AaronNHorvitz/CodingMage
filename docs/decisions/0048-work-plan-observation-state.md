# ADR 0048: Show Work Plan Outcomes Only From Live Status

- **Status:** Accepted
- **Date:** 2026-09-30
- **Decision owners:** CodingMage implementation

## Context

Work plan kept the last coordinator task outcomes after a status refresh failed
or aged. A small stale label sat above rows that still carried completed and
accepted badges. This could make a retained observation look like current
verification, especially when a long plan was scanned row by row. Empty and
malformed-plan views also ended without an in-screen recovery destination.

## Decision

Keep source checkboxes as source claims. On Work plan, show coordinator outcome
badges and selected source detail only after a completed, live, selected-campaign
status observation. During a refresh, after failure or after staleness, label
outcomes unknown and explain the cause; retain the old value internally for
other explicitly labelled history views. Offer an explicit `campaign-status`
refresh with its exact adjacent Show-command preview. Refuse the action when
the binary or path cannot be represented safely. Give an unstarted campaign,
no project and malformed task source separate explanations and navigation to
Setup or offline Help where applicable.

Give unknown badges task-specific visible labels. A software-rendered
diagnostic showed repeated generic unknown badges, but the accessibility tree
did not expose those repeated labels. Each unknown badge now names its item,
and the focused test checks the selected item's label.

## Consequences

The Work plan may temporarily show unknown outcomes during a coordinator read
even when it holds older records. This is a presentation choice; it neither
deletes coordinator evidence nor changes campaign authority. A successful
bound status response restores current outcome badges. The wider screen-state,
depth, localization, human accessibility and performance work remains open.

No dependency, licence, command contract or coordinator writer changes.

## Verification

See [the Work plan evidence](../evidence/sprint-36-workplan-state-recovery.md).
