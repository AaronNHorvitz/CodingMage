# ADR 0033: Native preflight failure and admission boundary

- **Status:** Accepted
- **Date:** 2026-09-29
- **Decision owners:** CodingMage implementation

## Context

The native interface retains the last successful preflight report when a later request fails.
That report is useful for inspection, but the admission control previously passed it to the
admission recorder even while a refresh was pending or after the refresh failed. An installed
coordinator file without execute permission was also reported as a generic process failure.
Together, these states hid the cause and could record an admission from an outdated observation.

## Decision

Classify operating-system execution permission refusal as a distinct backend error, retaining
only its stable code and no process output. The Preflight view says what happened, the effect on
admission, and the local recovery action. A retained report stays inspectable. The native
admission action is disabled and its direct method refuses while preflight is pending, failed or
stale under the existing observation window. A successful fresh preflight restores the action.

This is a client admission boundary. The coordinator still revalidates campaign authority when
it starts, and a read-only preflight never starts or authorizes a campaign. No new command,
provider, credential path or authority source is introduced.

## Consequences

An owner who takes longer than the current observation window to review a report must rerun
preflight before admitting it. This is a deliberate conservative state rule; the retained report
and digest remain available for comparison. Other screen states and human desktop checks remain
open under Task 36.3.2.2 and Story 36.3.
