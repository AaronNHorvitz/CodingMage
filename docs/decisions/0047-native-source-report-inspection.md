# ADR 0047: Inspect Source-Bound Reports Separately From Local Observations

- **Status:** Accepted
- **Date:** 2026-09-30
- **Decision owners:** CodingMage implementation

## Context

The native Reports screen offered a local schema-three observation assembled from
several UI reads and a source-bound schema-one export through the public
`codingmage report-export` command. The export still waited for the local
observation worker, even though the command reads its own fresh bound sources.
The user could not inspect the source-bound document in the interface before
choosing an export destination.

## Decision

Add an explicit read-only `campaign-outcome-report` action with the exact
in-context Show command. Its command always requests omission of changed-file
paths. Run it on an isolated bounded supervisor so a slow read cannot hold
campaign controls or window shutdown. Accept only a schema-one document with
the selected campaign, repository, authority digest and initial commit, and
with changed-file paths omitted. Discard stale, duplicate and foreign responses;
accept at most 256 KiB for inline parsing, bound the displayed JSON preview
to 4,096 characters and label it as a snapshot.

Keep the local schema-three observation visibly separate. Move the source-bound
export controls ahead of local assembly and remove their local-ready condition.
The CLI remains responsible for rebuilding the export document and enforcing
destination containment, path choice and overwrite. A failed or stale local
observation cannot authorize, block or contaminate that fresh CLI result.

## Consequences

The source report may fail independently when the coordinator is unavailable,
its output exceeds the UI capture limit, or its payload is malformed. The
screen names that failure and requires another explicit inspection; it never
turns a retained snapshot into a live state. A source-bound export may succeed
while local UI observations are still assembling or stale; its receipt and
fresh record set remain distinct from the local preview. The broader
section-five state, depth and action catalogue remains open.

## Verification

See [the implementation evidence](../evidence/sprint-36-native-source-inspection.md).
