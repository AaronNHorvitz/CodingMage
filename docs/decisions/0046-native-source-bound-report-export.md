# Decision 0046: Native Source-Bound Report Export

- **Status:** Accepted for an incremental Reports action
- **Date:** 2026-09-30
- **Decision owners:** CodingMage implementation lane under Decision 0020
- **Supersedes:** The native export action in Decisions 0038, 0040 and 0041
- **Superseded by:** None

## Context

The Reports screen formerly sent a UI-assembled schema-three document to its private
writer. Decision 0044 added public source-bound `campaign-outcome-report` and
`report-export` commands, but the native button did not use them. Its local observation
could be stale and was not the coordinator's current report projection. The action
also lacked an exact command preview.

## Decision

The native Export report button constructs one exact argument vector for
`codingmage report-export`: selected absolute configuration and campaign files,
destination, path-inclusion choice and overwrite choice. The same vector appears in
the in-context Show command control. A command that cannot be shown safely cannot
be submitted. Export remains an explicit action; opening Reports reads no new
authoritative state or writes no file.

The bounded backend queue dispatches report export to an isolated supervisor thread,
which invokes the selected coordinator binary with a deadline and selection-generation
cancellation. A blocked filesystem call can hold that supervisor and refuse a second
writer, but does not hold the normal control worker or window shutdown. The native
screen accepts success only after a strict version-one receipt names the selected
campaign and repository, confirms a nonempty write and matches the requested path
privacy. A malformed receipt leaves the destination uncertain and requires inspection.
The UI-assembled schema-three JSON remains available as a labelled local observation,
not a preview of the exported source-bound schema-one file.

## Consequences

The public coordinator owns report construction and guarded file publication for the
native action. It may read newer records than the local observation shown nearby;
the screen states that difference. The legacy private writer remains in the crate for
its bounded safety tests and can be removed in a later cleanup. The coordinator's
stable refusal code does not identify which destination check failed, so recovery
guidance asks the owner to inspect the destination and command inputs. The same-mount
rule and concurrent local filesystem race described in Decision 0045 remain.

This does not complete the section-five state/action catalogue, source-backed inline
inspection, human accessibility trials, live-provider qualification or release.
