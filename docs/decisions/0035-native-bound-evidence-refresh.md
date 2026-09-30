# ADR 0035: Bound native evidence refresh to live campaign status

- **Status:** Accepted
- **Date:** 2026-09-29
- **Decision owners:** CodingMage implementation

## Context

The Changes and evidence screen displayed coordinator projections but required a visit to the
Campaign screen to request them again. Its failed reads gave a generic error, and it did not
offer the exact command beside each read. A change projection requires the current campaign
head, while run evidence is checked against the current status checkpoint.

## Decision

Offer separate read-only refreshes for candidate changes and run evidence. Build each submitted
argument vector with the same function used by its Show command preview. Enable a manual read
only when the selected campaign status is live, the exact command is safely displayable and that
read is not already pending. Keep the existing generation, repository, campaign, head and status
checkpoint checks on responses. Label missing, loading, failed and stale observations explicitly;
state that an unavailable read proves no review, completion or delivery.

These controls do not admit, start, modify or publish a campaign. The coordinator owns the reads
and all authority. The broader state catalogue and full retained findings, logs and diff hunks
remain open until a bounded coordinator contract exists for them.
