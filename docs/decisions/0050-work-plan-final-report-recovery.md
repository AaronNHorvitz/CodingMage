# ADR 0050: Recover Final Report Observations in Work Plan

- **Status:** Accepted
- **Date:** 2026-09-30
- **Decision owners:** CodingMage implementation

## Context

Work plan's accepted badges depend on the coordinator's final report, but a
failed report refresh had only the shell's broader F5 recovery. The screen did
not explain why retained accepted outcomes disappeared or show the command that
would read the report again.

## Decision

Give Work plan an explicit read-only **Refresh final report** action for a
parallel campaign. Build its displayed command and bounded worker request from
the same selected configuration and campaign arguments. Continue using the
coordinator's `campaign-report` command and the existing response and payload
binding. A pending, failed or aged report observation does not provide an
accepted badge; report absence stays distinct from failure. Show the report's
freshness, the effect of a failure, and the next action in place.

## Consequences

The UI gains no authority over campaign execution and no second report source.
Serial campaigns have no final-report control. The wider section-five state,
depth and action catalogue remains open under Task 36.3.2.2.

## Verification

See [the Work plan report recovery evidence](../evidence/sprint-36-workplan-report-recovery.md).
