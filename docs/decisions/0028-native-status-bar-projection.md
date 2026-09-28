# Decision 0028: Show Bound Coordinator State in the Native Status Bar

- **Status:** Accepted for local implementation
- **Date:** 2026-09-28
- **Decision owners:** CodingMage implementation worker under the owner's delegated UI scope
- **Supersedes:** None
- **Superseded by:** None

## Context

The native UI specification requires repository, campaign, state, involvement, active pods,
current gate and last backend update on every screen. The shell previously showed only the
coordinator connection and an optional message. The coordinator's current `campaign-status`
output has a campaign state, active tasks with optional pod identities and a last checkpoint
timestamp. The admitted mission output has an involvement value. Neither command reports a
current gate. A synthesized gate could mislead the owner about test or review progress.

## Decision

Render the selected repository and campaign in a bottom panel on every screen. Show the
repository's short directory label and an abbreviated identity in the panel, with the full
selected path and selected or diagnosed identity on hover. Read state and active pod identities
only from a selected campaign's bound `campaign-status` observation. Count distinct nonempty
reported pod IDs and label that count as identified pods. Read involvement only from the
separately bound mission observation. Show no-charter, not-started, loading, failed and stale observations
separately; retained values are explicitly marked stale. Display the coordinator's exact
checkpoint time as milliseconds since 1970 UTC, with no inferred locale or clock adjustment.

Show “Current gate: not reported by coordinator” until a versioned coordinator command provides
a bound current-gate field. Do not infer a gate from a run record, an active task, or a local
build. The status bar performs no new authority action, filesystem read or subprocess request.

## Consequences and limits

This supplies a truthful always-visible overview from existing outputs without changing the
runtime schema or adding a dependency. The full section 5 state catalogue, locale-aware date
formatting, detailed activity log and human desktop accessibility/space measurements remain
separate open work. No backend gate is claimed to exist merely because the UI names the gap.
