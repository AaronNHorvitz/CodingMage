# Decision 0030: Bounded Task Source Detail at a Campaign Head

- **Status:** Accepted for incremental native UI implementation
- **Date:** 2026-09-29
- **Decision owners:** CodingMage implementation worker under the owner's delegated UI scope
- **Supersedes:** None
- **Superseded by:** None

## Context

The Work plan shows source titles, anchors and checkbox states, but its detail panel does not
show the source prose or story criteria. Reading a campaign-head task file from the native
client would bypass the coordinator boundary in Decision 0016. The active checkout may differ
from the reconciled campaign head.

## Decision

Add an explicit, read-only `campaign-task-detail` command with configuration, campaign, full
head and one parsed item ID. Reuse the authorized Git blob and strict plan parser used by
`campaign-head-plan`. Require the status head before and after the read to match the requested
head. Select exactly one parsed item; ambiguous and unknown IDs fail. Return a versioned response
with campaign and repository identities, head, full source digest, item line and line digest,
literal checkbox, a UTF-8-safe source excerpt capped at 16 KiB, and parsed story criteria.
The excerpt ends before the next parsed item or level-two/level-three heading. Return at most
100 criteria with 4-KiB titles; every cap has an explicit truncation flag.

The UI requests this only after an owner selects a task and presses **Load source detail**.
Its nearby **Show command** presents the exact invocation. The worker runs off the UI thread;
the UI checks request identity, current task selection, campaign, repository and live head
before rendering the response as inert content. Browsing the Work plan alone does not run the
command or change a campaign. This command exposes deliberately selected task prose to the
local caller, so it is not part of the content-minimized support bundle.

## Limits

The source excerpt and source-checkbox/criterion text are not verified completion. Packet,
attempt, review and test evidence remains in separate coordinator records and may be absent.
The status check establishes an observation at command completion, not a lease on a mutable
campaign head. A later status change causes the UI to hide the old detail.
