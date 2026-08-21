# Sprint 22 Serial Recovery Evidence

- **Status:** Reconciled-head, interrupted-integration, correction-session recovery, distinct
  serial queue projection, and closed stopping-condition contract pass; initial-implementation
  recovery and the complete Sprint 22 gate remain open
- **Recovery implementation commit:** `f05f033`
- **Queue-projection implementation commit:** `4745357`
- **Stopping-contract implementation commit:** `9816043`
- **Status-round implementation commit:** `442eb27`
- **Campaign-journal foundation commit:** `e3465f1`
- **Provider-capacity recovery test commit:** `c8efa11`
- **Fail-closed checkpoint-schema commit:** `a566021`
- **Outcome-projection implementation commit:** `b28a358`
- **Unit-utilization implementation commit:** `a8b9c62`
- **Campaign-utilization aggregation commit:** `2d72917`
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

Resume of a planning-time provider pause is available through the same exact campaign invocation
after access is restored. The binary fixture makes the Codex lead return a quota error on the first
invocation and an authentication-expiration error on the second. Each invocation ends in `paused`
with `capacity_pause`, zero accepted units, no current task, the exact content-free provider code,
and an unchanged active-checkout head and task source. A later invocation revalidates access by
successfully starting the provider, resumes the same campaign authority, and completes the existing
interruption-and-recovery workflow. CodingMage does not inspect, retain, or refresh credentials.

Correction-session interruption now resumes through the exact integrity-bound run, worktree,
candidate, round, and provider session. Initial implementation interruption remains blocked from
automatic replay.

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

## Closed Stopping Contract

Every successful campaign invocation now emits one typed `stop_reason` from a closed set:
completion, authenticated operator cancellation, capacity pause, accepted-outcome limit,
bounded-attempt limit, no independently safe ready work, or terminal policy failure. Campaign
state, stop reason, and content-free diagnostic are constructed as one internal termination value,
so a call site cannot omit the reason or accidentally shift it into another argument.

Current production exits explicitly classify canonical completion, the accepted-unit ceiling,
provider quota or authentication capacity, exhausted provider/correction/report attempts, absence
of independent ready work, and repository, authority, integrity, or policy refusal. CLI fixtures
verify serialized reasons for completed, blocked, deferred, correction-limit, and rejected-report
runs. Authenticated cancellation is reserved in the closed contract; its state-changing control
path remains truthfully open under sub-task `22.2.2.1` and is not claimed by this evidence.

## Privacy-Safe Round Status

`campaign-status` now includes `current_round` alongside phase, actor, current and last task,
completed count, blocker count and code, and elapsed time. The value is `null` at a clean boundary,
zero for an active unit before an accepted correction checkpoint, and otherwise the exact round
loaded from the integrity-verified private correction checkpoint. It is never inferred from a
transient progress label or provider statement.

Binary fixtures verify round zero during interrupted integration and at the deliberate boundary
immediately before the first correction checkpoint becomes durable, plus `null` after completion
or a clean pause. Status remains read-only and excludes prompts, source text, filenames, provider
prose, command output, environment values, and credentials. Model identity, independent attempt and
limit utilization, and monitor noninterference remain open under Story `22.2`.

## Campaign Journal Foundation

Every successful private campaign-checkpoint replacement now appends a typed projection to the
existing integrity-checked, append-only SHA-256 journal. Each record binds the campaign run,
repository, worktree, branch, reconciled head, exact active or last task, phase, completed count,
blocked/deferred/satisfied-trigger/human-decision/rejected counts, accepted-outcome count, and a
digest of the canonical checkpoint bytes. The record carries explicit redaction markers for
provider output, source text, command output, environment values, and credentials; those values are
not accepted into the event schema.

The journal schema uses optional fields for configured limits, provider attempts, correction round,
and authenticated pause/stop/cancel state. They remain `null` in this foundation because their
campaign-owned counters and control authority are not implemented yet. This commit therefore does
not close `22.1.2.1`; it supplies the typed append path that the remaining Sprint 22 safeguard tasks
will populate and reconcile. Focused tests verify exact identities and counts, checkpoint-digest
evidence, redaction markers, journal-chain validity, and rejection of contradictory accepted counts.

## Fail-Closed Checkpoint Schema

Campaign checkpoints now accept only the complete schema-2 projection. The production loader no
longer migrates an older shape by supplying empty collections for state that the stored digest did
not authenticate. Five fixtures construct correctly hashed historical shapes that successively lack
blocked tasks, typed blocker reasons, deferrals, human-decision holds, and rejected-proposal history;
every one is refused with no recovery effect. A current checkpoint still round-trips exactly and a
single-byte mutation still fails integrity validation.

This deliberately invalidates restart from pre-schema-2 campaign checkpoints. An operator must
inspect and explicitly start a new campaign authority from a verified repository state instead of
allowing CodingMage to infer that absent safety state was empty. The remaining utilization and
operator-control fields will be required members of this same closed checkpoint shape rather than
backfilled through a permissive migration.

## Outcome Projections And Ceiling

Schema-3 checkpoints contain independent completed, blocked, deferred, pending-human-decision,
rejected-proposal, accepted-outcome, and maximum-accepted counters. Before every replacement, the
coordinator derives those counters from the canonical durable collections and refuses overflow or
an accepted count above the operator-authorized ceiling. On restart, it verifies every stored count
against those collections instead of normalizing a mismatch.

The campaign admission boundary now reads the validated accepted-outcome projection. Completed,
blocked, deferred, and pending-human-decision outcomes each consume one unit; rejected lead
proposals remain historical evidence and consume no accepted-outcome capacity. The journal records
the same exact counters and the configured maximum rather than leaving `max_outcomes` absent.

Focused mutation coverage changes each outcome counter independently, recomputes a valid outer
checkpoint checksum, and proves that semantic reconstruction still refuses the checkpoint. Existing
restart coverage also preserves each disposition separately, including a deferred task's typed
reason, exact reconsideration trigger, source head, task-source digest, and whether that trigger was
already observed.

## Utilization Ledger Progress

Each supervised unit now returns a content-free utilization record containing attempted
provider invocations, metadata-report repairs, provider and gate process invocations, full observed
output-byte totals where receipts exist, observed execution milliseconds, and exact terminal bytes
beneath its private run-state root. Provider attempts are incremented before invocation. Arithmetic
overflow, unreadable state, a symbolic link, or a non-file entry beneath the run-state root fails
closed.

The binary workflow fixture injects one distinctively large malformed Claude completion response before any edit and
then completes two bounded correction rounds. It verifies exactly six provider attempts, one
metadata repair, ten provider-plus-gate processes, output exceeding the malformed response's byte
count, and nonzero retained state while preserving the same reviewed completion result. Claude and
Codex adapters retain the process receipt separately from provider-result interpretation, so quota,
authentication, timeout, malformed-output, and other post-execution failures contribute their
available output and elapsed observations without persisting provider content.

Schema-5 campaign checkpoints aggregate successful and failed unit receipts plus lead attempts and
all available lead-process observations. Each checkpoint records provider attempts, malformed-report
repairs, correction rounds, process invocations, output bytes, retained campaign-state bytes, and
observed execution milliseconds alongside each operator-authorized maximum; the hash-chained journal
projects the same complete tuple and rejects partial records or count consumption beyond a reserved
count ceiling. Atomic output, elapsed, or retained-state observations may truthfully exceed their
ceiling and remain durable so the next effect is refused with the exact reason.
The copied limits and accepted-outcome ceiling must also match the current integrity-verified
campaign authority on status, control, and resume operations.

Campaign-spec schema 3 independently authorizes provider-attempt, malformed-report-repair,
correction, process, output, retained-state, and elapsed-execution ceilings. The campaign checks
exhaustion before selecting another unit. Inside a unit, the same authority reserves each provider
process, metadata repair, correction, and complete deterministic gate set before that effect starts.
Provider exhaustion cannot skip an already-authorized gate; a gate set that would exceed the process
ceiling does not start partially. Output, retained-state, and elapsed observations stop the next
effect after the atomic process that established the observation.

Focused boundary tests permit the exact authorized count and reject the next effect for all seven
limit classes. A production binary fixture reaches one aggregate correction round, refuses the
second correction before writing its intent, returns
`codingmage.campaign.limit.correction_rounds`, preserves the active checkout, and retains the latest
reviewed candidate branch for inspection. The exhaustive minimum, maximum, one-below, one-above,
overflow, restart, and concurrent-observation matrix remains open under sub-task `22.2.3.5`.

## Verification

The complete workspace test suite passed across all targets, including 31 runtime unit tests and
eight CLI workflow tests. Strict workspace Clippy with warnings denied, workspace formatting,
workspace documentation generation, architecture checks, documentation policy, 12 Python policy
tests, and `git diff --check` also passed after aggregate-limit enforcement and evidence
reconciliation.

A preliminary disposable one-pod soak ran the binary interruption-and-recovery campaign five times
from fresh fixture repositories. All five cycles passed. The post-soak check found no fixture
directories, provider processes, or extra CodingMage worktrees. Each cycle covered an interrupted
integration, restart reconciliation, two dependency-ordered reviewed units, completion adoption
without replay, durable status, and active-checkout preservation.

## Preserved Limits

This evidence does not close initial-implementation-session recovery, complete campaign event
journaling, production operator
controls, parallel pods, GitHub publication, native macOS or Windows evidence,
manual fuzzing, or the sustained 24-hour or 48-hour soak gate. It authorizes only preparation of the
explicitly requested bounded one-pod controlled-target pilot; it does not assert general
unattended-release readiness.
