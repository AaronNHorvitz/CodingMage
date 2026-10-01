# ADR 0049: Bind Native Final Reports Before Showing Accepted Outcomes

- **Status:** Accepted
- **Date:** 2026-09-30
- **Decision owners:** CodingMage implementation

## Context

The native client parsed the parallel campaign's final `campaign-report` but did
not compare its payload identity with the selected campaign specification. A
request could have the correct generation and path binding while its output
named a different campaign or repository. Work plan also combined a retained
report with a newer status observation without checking whether they described
the same final head. A failed report refresh retained the earlier value.

## Decision

Compare a non-null report's campaign ID, repository ID, starting commit and
branch prefix with the selected validated specification before accepting it as
an observation. A mismatch is a contract error. Keep a null report distinct
from malformed output when a campaign remains selected.

Show a report-derived accepted outcome on Work plan only when the selected
status is completed and live, no status or report request is pending, the
report's latest refresh did not fail, and status and report agree on campaign,
branch and final head. Source checkboxes remain separate. Retained observations
can still be described as stale in Reports; a retained report does not become
a current Work plan outcome by itself.

## Consequences

The native client does not verify the report's underlying coordinator journal
or mint a completion claim. The coordinator remains the authority for the
report; this is a consumer-side binding and presentation guard. The report
command, writer, provider, dependencies and campaign permissions do not change.
Other section-five states and in-context depth remain under Task 36.3.2.2.

## Verification

See [the report-binding evidence](../evidence/sprint-36-report-payload-binding.md).
