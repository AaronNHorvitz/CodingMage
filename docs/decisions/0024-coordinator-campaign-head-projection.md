# Decision 0024: Coordinator Projection of Campaign-Head Task States

- **Status:** Accepted for incremental native UI implementation
- **Date:** 2026-09-28
- **Decision owners:** CodingMage implementation worker under the owner's delegated UI scope
- **Supersedes:** None
- **Superseded by:** None

## Context

The native workspace previously ran `git show` directly to read the task source at a campaign
head. That observation bypassed the coordinator's campaign and repository authorization, and
returned the full task text through the UI worker. The active checkout can have a different task
source from the reconciled campaign head; showing those checkbox states as one state would be
misleading.

## Decision

Add `codingmage campaign-head-plan --config ... --campaign ... --head ...` as a read-only,
versioned coordinator projection. The command loads the exact campaign authority and durable
status, requires the caller's full commit ID to equal the reconciled head, authorizes the held
repository, and reads only the configured task-source blob through the hardened Git command
template. The same eight-MiB task parser validates the blob. The command rechecks durable status
after parsing and returns only campaign/repository identities, the exact head, the source digest,
and literal sub-task IDs and checkbox states. A changed head fails with a stable stale-observation
code. The UI checks the response against its current selected campaign, repository and head before
building an overlay. It never treats a checked source box alone as verified completion.

The Git blob capture limit is eight MiB for this command because that is the parser's source
limit. Other Git observations retain their four-MiB limit. The UI's command output remains capped
at eight MiB. No provider, task, or campaign is started by this projection.

## Verification and limits

A disposable fake-provider campaign exercises a checked task at the reconciled head and a stale
head refusal, while the parser tests duplicate IDs, unknown fields and future schema versions.
The UI's other direct Git reads and private run-record observation remain open under Task
36.3.2.2. A source mutation after the command's final status check can occur; the UI's repeated
status polling and response binding detect a subsequently observed head change.
