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
- **Control-journal reconciliation commit:** `96a0e8b`
- **Exact-descendant cancellation commit:** `f19c33e`
- **Stop-after-unit evidence commit:** `800c215`
- **Resume-revalidation implementation commit:** `d860194`
- **Durable pending-resume commit:** `a6fe0a9`
- **Control-boundary restart matrix commit:** `4511b54`
- **Limit-policy invariance commit:** `aaa1a87`
- **Exhaustive limit-matrix commit:** `3c92e82`
- **Complete status projection commit:** `87f2438`
- **Typed task-status commit:** `3ad93a1`
- **Status privacy-schema commit:** `e72a5bb`
- **Observational monitor implementation commit:** `b1aaba7`
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
reporting only phase, actor category, a model identity when the durable phase proves the exact
configured owner, branch, reconciled head, current and last task identifiers, current correction
round, aggregate attempt count, independent outcome counters, current utilization and configured
ceilings, blocker count and code, elapsed time, and update time.

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

Current production exits explicitly classify canonical completion, operator pause,
stop-after-unit, authenticated cancellation, the accepted-unit ceiling,
provider quota or authentication capacity, exhausted provider/correction/report attempts, absence
of independent ready work, and repository, authority, integrity, or policy refusal. CLI fixtures
verify serialized reasons for completed, blocked, deferred, correction-limit, and rejected-report
runs. Focused runtime tests verify the distinct stop and cancellation states. A production campaign
fixture cancels a sleeping lead, verifies prompt exact-group cleanup, preserves a concurrent
unrelated process, and returns the authenticated terminal cancellation code.

## Privacy-Safe Round Status

`campaign-status` schema 2 includes `current_round` alongside phase, actor, current and last task,
completed count, blocker count and code, and elapsed time. The value is `null` at a clean boundary,
zero for an active unit before an accepted correction checkpoint, and otherwise the exact round
loaded from the integrity-verified private correction checkpoint. It is never inferred from a
transient progress label or provider statement.

Binary fixtures verify round zero during interrupted integration and at the deliberate boundary
immediately before the first correction checkpoint becomes durable, plus `null` after completion
or a clean pause. A concurrent planning-phase status read proves the `codex-lead` actor, exact
configured lead model, and nonzero attempt count; terminal status reports no model rather than
guessing across an ambiguous phase. The same fixture verifies every outcome, utilization, and limit
object is present and internally consistent. Status remains read-only and excludes prompts, source
text, filenames, provider prose, command output, environment values, and credentials.

Sorted blocker entries contain only canonical task identity and a closed blocker-reason code.
Sorted deferral entries contain only canonical task identity, a closed reason and trigger code, and
the closed `pending` or `satisfied` trigger state. Human-decision holds use the same task/reason-only
shape. Checkpoint loading now refuses mismatched blocker IDs and reason maps, overlapping open
dispositions, invalid task identities, pending/satisfied overlap, and reason/trigger mismatch before
status can serialize them. Production CLI fixtures prove pending-to-satisfied deferral movement and
exact blocker removal after authenticated clearance. The prohibited-content mutation corpus and
monitor noninterference remain open under `22.2.4.4`.

The prohibited-content corpus injects unique prompt, source-text, filename, provider-prose,
command-output, environment-value, credential, and hidden-reasoning markers into recovery-only
fields. It proves none cross into status JSON or the append-only checkpoint journal. The status
schema denies unknown fields at its top level and in every nested projection; mutation tests add
each prohibited class plus a nested command-output field and require deserialization refusal.
Recovery checkpoints may retain exact authorized paths and hashes needed for safe reconciliation,
but public status and durable journal records do not copy them.

`campaign-explain-blocker` is now a separate privacy-safe projection of the same validated campaign
status read. It returns only campaign identity, phase, the campaign-level blocker code, canonical
task identities, closed blocker or deferral reason codes, closed reconsideration triggers and their
state, and human-decision holds. It has no checkpoint writer, provider adapter, process runner, Git
mutation, or lifecycle-control handle.

The production blocked-campaign fixture treats separate CLI processes as attach, disconnect, and
reconnect boundaries. Across sixteen status polls and sixteen blocker-explanation reconnects, it
requires byte-identical campaign state and scratch trees, exact active-checkout head and porcelain
state, identical complete ref inventory, unchanged lead and implementer logs, stable typed blocker
output, and an unchanged provider-attempt count. The monitor crate separately proves reconnectable
event cursors and all read commands are observational. Together these close `22.2.4.4` and the
complete privacy-safe campaign-status task without claiming that the later attachable TUI exists.

## Campaign Journal Foundation

Every successful private campaign-checkpoint replacement now appends a typed projection to the
existing integrity-checked, append-only SHA-256 journal. Each record binds the campaign run,
repository, worktree, branch, reconciled head, exact active or last task, phase, completed count,
blocked/deferred/satisfied-trigger/human-decision/rejected counts, accepted-outcome count, and a
digest of the canonical checkpoint bytes. The record carries explicit redaction markers for
provider output, source text, command output, environment values, and credentials; those values are
not accepted into the event schema.

The journal schema uses optional fields for configured limits, provider attempts, correction round,
and authenticated pause/stop/cancel state. The current checkpoint writer populates the complete
limit and utilization tuple and projects applied pause, stop, and cancellation state. This does not
yet close `22.1.2.1`: exact control intent/observation crash reconciliation and the remaining actor
and terminal-state coverage must still be completed. Focused tests verify exact identities and
counts, checkpoint-digest evidence, redaction markers, journal-chain validity, and rejection of
contradictory accepted counts or partial control projections.

## Operator Control Inbox

Checkpoint schema 7 includes `operator_paused`, `stop_after_unit`, `cancelled`, the closed
`resume_validation` state, and the ordered set of applied control request IDs. A same-user caller
can submit only `pause`, `resume`,
`stop_after_unit`, or `cancel` through a create-once, integrity-bound request file beneath the
campaign's private state root. The request binds the exact repository, campaign run, authority,
action, request ID, and observed campaign head. Requests are consumed in a stable timestamp and ID
order. Exact replay has no second effect; conflicting reuse, a symbolic link, loose permissions,
wrong ownership, unknown action, cross-run authority, stale identity, malformed content, or a
partial checkpoint projection fails closed.

The coordinator remains the only checkpoint writer. Focused state-machine tests verify pause,
resume, stop-after-unit, and cancel transitions and their distinct serialized termination mappings.
The production binary campaign fixture submits the same pause twice, proves no provider advances
while paused, then applies a separate resume before campaign work continues.

The durable request file is the control intent and the checkpoint is the applied effect. A distinct
`ControlRequested` event records the former and `ControlApplied` records the latter. On every
request replay and campaign boundary, reconciliation validates the complete journal chain against
the exact inbox and checkpoint. It appends only a missing request or applied observation and never
reapplies the state transition. Duplicate observations, applied-without-request evidence, unknown
requests, cross-run records, conflicting actions, and authority mismatches fail closed.

A campaign-scoped watcher reads only integrity-valid requests for its exact authority and propagates
cancel through inherited process tokens. Lead, implementer, reviewer, capability probe, and gate
process groups are terminated and reaped by their existing exact guards. Gate-local required-failure
cancellation remains a child token and cannot propagate upward as an operator cancellation. A live
binary fixture proves a sleeping lead terminates promptly while a concurrently running unrelated
process and the active checkout remain untouched. The serial interruption fixture also submits
stop-after-unit during implementation, proves the reviewed unit reconciles exactly once, admits no
second unit, and continues only after a separately identified resume.

Resume now creates a durable `pending` validation state before it clears an operator pause or stop.
Before admission, the coordinator revalidates active-repository cleanliness and identity, the exact
campaign worktree and head, campaign authority, task parsing, queue and deferred-trigger
projections, accepted-outcome and aggregate limits, and all configured provider capability
surfaces. Probe subprocesses are guarded and their actual process, output, and elapsed receipts are
metered, including receipts collected before a later probe fails.

The serial binary fixture deliberately removes required Claude capabilities after resume. The
campaign refuses admission and persists `resume_validation = pending`; after restart and capability
restoration, the same campaign must repeat and pass the complete proof before the lead can run.

A deterministic restart matrix covers pause, resume, stop-after-unit, and cancel at each durable
boundary: after the intent file, after `ControlRequested`, after the integrity-bound checkpoint
effect, and after `ControlApplied`. All sixteen cases recover to the exact action state, retain one
request identity, contain exactly one request and one applied journal observation, remain unchanged
through two additional replays, and load from the final checkpoint with its digest and outcome
projection intact. Resume is reported as newly applied only when the crash preceded checkpoint
persistence; later replay preserves its pending validation prerequisite without repeating the
effect. This closes `22.2.2.1` through `22.2.2.5` and the parent campaign-control task.

## Fail-Closed Checkpoint Schema

Campaign checkpoints now accept only the complete schema-7 projection. The production loader no
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
metadata repair, fifteen capability-probe, provider, and gate processes, output exceeding the malformed response's byte
count, and nonzero retained state while preserving the same reviewed completion result. Claude and
Codex adapters retain the process receipt separately from provider-result interpretation, so quota,
authentication, timeout, malformed-output, and other post-execution failures contribute their
available output and elapsed observations without persisting provider content.

Schema-7 campaign checkpoints aggregate successful and failed unit receipts plus lead attempts and
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

Every nested provider executable, model, and effort selector; implementation-authentication mode;
gate-tier name and complete profile membership; allowed and denied path set; protected branch; and
publication mode is part of the canonical authority digest. A focused matrix exhausts each of the
seven aggregate ceilings plus the accepted-outcome cap, persists and reloads the checkpoint, and
proves that weaker provider effort, reduced gate membership, changed paths, or broader publication
cannot validate against the retained authority. Required gates continue to require
`OutputNotTruncated`, so an output ceiling cannot convert partial diagnostics into passing evidence.

Focused boundary tests permit the exact authorized count and reject the next effect for all seven
limit classes. The complete matrix verifies the minimum and maximum accepted authority, one-below,
exact-boundary, one-above, checked arithmetic overflow, integrity-checked restart reconstruction,
and four simultaneous read-only observations for every aggregate ceiling. Retained-state maximum
and overflow use the same extracted checked accumulator as the production filesystem walk, without
requiring an artificial enormous file. The accepted-outcome ceiling independently covers the same
configuration, persistence, overflow, restart, and concurrent-read properties.

A production binary fixture reaches one aggregate correction round, refuses the
second correction before writing its intent, returns
`codingmage.campaign.limit.correction_rounds`, preserves the active checkout, and retains the latest
reviewed candidate branch for inspection. This closes `22.2.3.1` through `22.2.3.5` and the parent
independent-limit task.

## Verification

The complete workspace test suite passed across all targets, including 40 runtime unit tests and
nine CLI workflow tests. Strict workspace Clippy with warnings denied, workspace formatting,
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
journaling, privacy-safe status completion, parallel pods,
GitHub publication, native macOS or Windows evidence,
manual fuzzing, or the sustained 24-hour or 48-hour soak gate. It authorizes only preparation of the
explicitly requested bounded one-pod controlled-target pilot; it does not assert general
unattended-release readiness.
