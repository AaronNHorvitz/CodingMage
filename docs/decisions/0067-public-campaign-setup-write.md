# ADR 0067: Publish Campaign Setup Through a Repository-Bound CLI Command

- **Status:** Accepted for local implementation; independent review pending
- **Date:** 2026-10-01
- **Decision owners:** CodingMage implementation

## Context

The native Setup screen still writes a campaign specification directly. The
existing public `setup-write-authorization` command and guarded report writer
provide a coordinator-owned publication boundary, but they cannot validate a
campaign's observed repository, task source and external authorization record.
Campaign authoring needs that binding before moving off the UI thread. The
campaign specification already has a strict versioned loader and verifier.

## Decision

Add `codingmage setup-write-campaign` with exact public arguments for the
configuration, observed repository ID, full Git HEAD, task-source SHA-256,
external authorization record, destination and explicit overwrite choice.
The candidate TOML is carried only on bounded stdin, capped at 1 MiB. Its
bytes go through the same strict campaign parser and verifier as file loads.
The command holds a repository authorization, checks the current clean Git
inventory and committed task-source blob against the submitted head and
digest, and checks the authorization record's bounded bytes and digest. Its
candidate must agree with all of those observations and the configured
repository path. The configuration, record and source observations are checked
again at the guarded publication boundary. A changed observation refuses
success; a change after publication retains the writer's uncertainty
reconciliation behavior. Configuration and authorization record destinations
cannot be overwritten as the campaign file, including through a parent alias;
the configuration cannot double as the authorization record.

The receipt contains the repository ID, campaign ID, head, byte count and
candidate digest. It contains no candidate TOML or authorization text. This
command writes a document; it does not admit or start a campaign, grant
publication or assert provider readiness. It does not accept a dirty worktree
as a valid task-source observation. The native screen and its Show-command
preview remain a separate follow-on increment.

No new dependency or licence was introduced; the CLI already uses the
campaign, Git, plan and SHA-256 crates at pinned workspace versions.

## Verification

The disposable real-process suite covers exact-byte publication, the strict
loader, a content-minimized digest receipt, no-overwrite and explicit replace,
cross-repository identity, stale head and task digest, invalid campaign
authorization digest, malformed/oversized input, unsafe destination,
configuration/record collision, dirty source and changed committed head. The
campaign parser unit test covers direct-byte parity and invalid input. Exact
results and open independent/human gates are in the Setup command evidence.

## Independent finding

The bounded exact-commit review of `4c4afe0fbe98dd96c2a45c6bab8cde10d5b4c283`
returned `FINDINGS` with a High hard-link alias defect. The record could be a
second name for the configuration file while supplying the matching candidate
digest. Decision 0068 defines the physical-file correction. The original
review remains attached to its exact source and does not approve the fix.
