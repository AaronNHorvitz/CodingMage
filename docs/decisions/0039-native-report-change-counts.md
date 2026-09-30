# ADR 0039: Distinguish unobserved and partial report changes

- **Status:** Accepted
- **Date:** 2026-09-30
- **Decision owners:** CodingMage implementation lane

## Context

The Reports screen assembles a document from independently refreshed coordinator projections.
Before `campaign-changes` returned, it supplied empty commit and file lists to the assembler.
The export then encoded a changed-file count of zero, which could be read as an observed result.
The coordinator can also truncate a successful change projection, so its list lengths need not
be complete totals.

## Decision

Report schema 3 carries `change_coverage.observed`, `change_coverage.commits_truncated` and
`change_coverage.files_truncated`. `changed_file_count` is nullable: `null` means the change
projection was not observed, zero means the coordinator actually returned an empty list, and a
number with `change_coverage.files_truncated=true` is a lower bound. An unobserved projection
supplies no commit or changed-file content even if an inconsistent caller passes nonempty slices.
The screen labels unobserved counts as such and truncated counts as lower bounds. Exported
limits explain the same distinction. A failed or old observation retains its existing
per-source freshness label; this change does not silently promote it to live.

## Consequences

Consumers of the interface-generated report must understand schema 3 and its nullable count.
The report remains a presentation of coordinator observations, not independent evidence or a
new coordinator record. Export still uses the UI's local writer on the interface thread;
Decision 0016 command equivalence and the native specification's off-thread requirement remain
open under Task 36.3.2.2. Human, live-provider and release gates are unaffected.

## Verification

See [the exact-source report count evidence](../evidence/sprint-36-report-change-counts.md).
