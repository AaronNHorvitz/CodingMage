# 0019: Main-First Development Publication

| Field | Value |
| --- | --- |
| Status | Accepted |
| Date | 2026-09-24 |
| Authority | Owner request to consolidate development into main |

## Decision

Adopt [main-first development](../../DEVELOPMENT-WORKFLOW.md). Consolidate committed
development history and accepted roadmap changes into main, preserving both sides of a
divergence. Keep unfinished source edits intact and separate from published snapshots.

This supersedes older local-only and development-branch-only instructions for development
of this repository. The designated publisher may perform ordinary fast-forward pushes to
main and necessary non-destructive local merges. It may not force-push, discard unique work,
weaken runtime safeguards, change licensing/visibility or claim independent release approval.

## Evidence and Consequences

main is the development integration branch, not a certified or supported release.
The owner-requested initial consolidation retains existing verification limitations.
Subsequent batches require exact-commit inspection and applicable checks before publication.
Isolated execution checkouts must publish completed batches through the assigned publisher,
not become indefinitely disconnected development branches.
