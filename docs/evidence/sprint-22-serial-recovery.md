# Sprint 22 Serial Recovery Evidence

- **Status:** Reconciled-head, interrupted-integration, correction-session recovery, and distinct
  serial queue projection pass; initial-implementation recovery, stopping conditions, and the
  complete Sprint 22 gate remain open
- **Recovery implementation commit:** `f05f033`
- **Queue-projection implementation commit:** `4745357`
- **Executed:** 2026-08-20 on Fedora Linux with Rust 1.95.0

## Durable Recovery

The serial campaign now atomically persists a private integrity-bound checkpoint containing the
original authority digest, repository and campaign identities, owned worktree and branch, initial
and reconciled heads, active task, pending integration intent, accepted-unit count, blocker, and
timestamps. A same-authority restart loads only the exact manifest-selected worktree and revalidates
its physical identity, registration, branch, lineage, cleanliness, and expected head.

The binary campaign fixture interrupts the coordinator after the reviewed child unit is complete
and after the integration intent is durable, but before the campaign fast-forward. Durable status
then reports `integrating`, the exact active task, and zero reconciled units. Restart reobserves
whether the campaign head is still the expected parent or already the reviewed target, performs or
recognizes the exact path-bounded fast-forward once, and completes the dependent unit. A subsequent
restart returns the identical completed outcome without starting a lead, implementer, reviewer, or
integration effect. The active checkout and canonical task source remain byte-identical throughout.

## Provider Capacity And Status

Claude and Codex adapters classify quota and authentication failures separately from generic
provider failure. Campaign mode converts those categories into durable content-free pause codes.
The read-only `campaign-status` command validates campaign authority and checkpoint integrity before
reporting only phase, actor category, branch, reconciled head, current and last task identifiers,
completed count, blocker count and code, elapsed time, and update time.

Automatic resume of a planning-time provider pause is available through the same exact campaign
invocation after access is restored. Correction-session interruption now resumes through the exact
integrity-bound run, worktree, candidate, round, and provider session. Initial implementation
interruption remains blocked from automatic replay.

## Distinct Queue Projection

Before each lead selection, the coordinator now derives the completed sub-task set from the exact
canonical task source at the reconciled campaign head and independently projects durable blocked,
deferred, and pending-human-decision task sets. Rejected lead proposals remain a separate ordered
historical projection and do not suppress otherwise ready work. Only the union of the three open,
unavailable task sets is passed to dependency-ready selection.

The coordinator fails closed if an unavailable projection names a missing or checked sub-task, if
blocked, deferred, and pending-human-decision sets overlap, or if a typed blocker reason lacks its
corresponding blocked-task identity. Focused restart coverage persists an integrity-bound checkpoint
containing one blocked, one deferred, one pending-human-decision, and one rejected-proposal outcome;
after reload, the checked task and all four durable outcomes remain distinct and the same fifth task
is selected. Separate hostile cases prove that checked-task suppression and overlapping projections
are refused rather than normalized.

## Verification

The complete workspace test suite passed across all targets, including 26 runtime unit tests and
eight CLI workflow tests. Strict workspace Clippy with warnings denied, workspace formatting,
workspace documentation generation, and `git diff --check` also passed for the queue-projection
implementation. Architecture and documentation policy are rerun after this evidence update.

A preliminary disposable one-pod soak ran the binary interruption-and-recovery campaign five times
from fresh fixture repositories. All five cycles passed. The post-soak check found no fixture
directories, provider processes, or extra CodingMage worktrees. Each cycle covered an interrupted
integration, restart reconciliation, two dependency-ordered reviewed units, completion adoption
without replay, durable status, and active-checkout preservation.

## Preserved Limits

This evidence does not close initial-implementation-session recovery, complete campaign event
journaling, exact limit projection, automatic authentication
revalidation, cancellation, parallel pods, GitHub publication, native macOS or Windows evidence,
manual fuzzing, or the sustained 24-hour or 48-hour soak gate. It authorizes only preparation of the
explicitly requested bounded one-pod AgentMage pilot; it does not assert general unattended-release
readiness.
