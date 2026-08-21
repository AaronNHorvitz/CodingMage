# Serial Campaign

## Boundary

`codingmage campaign` is the first live hierarchical-campaign rollout. It processes one pod at a
time from an isolated evolving campaign branch. It never modifies the active checkout, pushes a
branch, creates or approves a pull request, merges a GitHub pull request, publishes a release, or
changes a protected branch.

Each accepted unit follows this sequence:

1. Reparse the canonical task source from the exact current campaign head.
2. Compute a deterministic dependency-ready set and expose one task to the read-only Codex lead.
3. Validate the structured proposal against the exact head, source digest, dependencies, paths,
   gate tiers, resources, artifacts, and campaign authority.
4. Run the production Claude implementation, local gates, bounded correction, Codex review, final
   verification, checkpoint, and mechanical task-completion workflow.
5. Preview the complete changed-path set from the old campaign head to the reviewed completion.
6. Fast-forward only the coordinator-owned campaign branch when ancestry, paths, identity, and
   cleanliness still match.
7. Repeat from the new exact head until completion, the unit ceiling, or a truthful blocker.

The lead contract distinguishes `blocked`, `deferred`, and `human_decision_required` outcomes.
Typed blockers, authenticated blocker clearance, exact reconsideration triggers, authenticated
external-trigger observations, repeated-deferral escalation, and independent-work continuation are
implemented. Invalid lead output is durably refused without downstream effects. Human-decision
resolution, the complete campaign-level every-reason mutation gate, and the exact ten-outcome soak
remain unchecked roadmap work.

## Campaign Spec

The values below are illustrative. Repository identity, commit, source digest, paths, executables,
models, unit limits, and branch policy must be selected from the intended target and local installation.

```toml
version = 2
campaign_id = "example-campaign"
repository_id = "repo-example"
repository_path = "/absolute/repository"
initial_commit = "0123456789abcdef0123456789abcdef01234567"
task_source_sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
operator_authorization_sha256 = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
max_parallel_pods = 1
max_units = 10
implementer_authentication = "existing_login"
campaign_branch = "codingmage/example-campaign"
allowed_paths = ["crates", "docs", "tests"]
denied_paths = ["private"]
protected_branches = ["main"]
publication = "local_only"

[team_lead]
executable = "/absolute/path/to/codex"
model = "gpt-5.6-sol"
effort = "high"

[implementer]
executable = "/absolute/path/to/claude"
model = "opus"
effort = "high"

[reviewer]
executable = "/absolute/path/to/codex"
model = "gpt-5.6-sol"
effort = "high"

[[gate_tiers]]
name = "focused"
profiles = ["configured-gates"]
```

Allowed and denied roots must be disjoint repository-relative paths. Gate profile names identify
operator-authored policy; a model cannot supply shell commands. Omitted `max_parallel_pods`
defaults to one, and this rollout still admits only one proposal per generation when a higher ceiling
is explicitly present.

The campaign lead must express every ownership root relative to the repository root, never relative
to a crate, package, module, or current file. Each root must already be an exact regular file or
directory in the bound snapshot and cannot be a symbolic link. A proposal that needs to create a
file owns its existing parent directory and names the new file in `expected_artifacts`. The
coordinator validates these rules before invoking Claude. Claude receives repository-wide read
access and Edit/Write permissions only for the validated ownership roots. Coordinator-side Git
inventory remains authoritative and rejects any change outside those roots even if a provider-side
permission control fails.

## Invocation

```bash
cargo build --locked --release -p codingmage-cli
./target/release/codingmage doctor --config /absolute/codingmage.toml
./target/release/codingmage campaign \
  --config /absolute/codingmage.toml \
  --campaign /absolute/campaign.toml
./target/release/codingmage campaign-status \
  --config /absolute/codingmage.toml \
  --campaign /absolute/campaign.toml
./target/release/codingmage campaign-clear-blocker \
  --config /absolute/codingmage.toml \
  --campaign /absolute/campaign.toml \
  --task 21.2.2.5 \
  --request clear-21-2-2-5-1 \
  --prerequisite-sha256 "${PREREQUISITE_SHA256}"
./target/release/codingmage campaign-observe-trigger \
  --config /absolute/codingmage.toml \
  --campaign /absolute/campaign.toml \
  --task 21.2.3.3 \
  --trigger operator_resume \
  --request resume-21-2-3-3-1 \
  --evidence-sha256 "${TRIGGER_EVIDENCE_SHA256}"
```

The final JSON reports campaign identity, terminal state, retained local branch, exact head,
completed-unit count, last task, and a content-free blocker code when applicable. Live stderr adds
`codex-lead` and `integration` stages to the existing per-unit activity stream.

`campaign-status` validates the original campaign authority and checkpoint integrity before showing
only the durable phase, actor category, local branch, reconciled head, current and last task IDs,
the integrity-verified current correction round when active, completed count, blocker count and
code, and elapsed time. It does not expose prompts, provider output, source text, repository paths,
credentials, or diagnostics.

`campaign-clear-blocker` is an explicit same-user mutation. The campaign state directory and
checkpoint must remain owned by the invoking Linux user with no group or world access. The command
binds a fresh request ID to one blocked task, its typed reason, current campaign head, current task
source, and an operator-supplied digest of the changed prerequisite. It persists that intent before
removing the blocker, revalidates the repository and isolated worktree before each effect, and is
idempotent when repeated with the same fields. Conflicting reuse fails closed. No provider starts,
no task is completed, and no active-checkout byte changes during clearance.

`campaign-observe-trigger` applies the same local ownership, campaign-lock, repository, worktree,
head, task-source, and task-opening checks to one externally observed deferral trigger. Only
`provider_reset`, `review_completion`, and `operator_resume` are accepted through this command;
campaign-head advancement, lease release, and gate-resource release derive solely from coordinator
state. The command persists a create-once request bound to the exact typed deferral and an
operator-supplied evidence digest before returning that task to deterministic ready-set evaluation.
Exact replay reports no second change, while conflicting request reuse fails closed. It starts no
provider and changes no task checkbox or active-checkout byte.

The target status contract additionally exposes distinct completed, blocked, deferred,
human-decision, and rejected-proposal counts, exact trigger state, and independent limit
utilization. It remains incomplete until Story 22.2 passes.

Malformed or unauthorized lead output pauses with either
`codingmage.campaign.lead_rejected.malformed_output` or
`codingmage.campaign.lead_rejected.invalid_proposal`. The integrity-protected rejection projection
contains only its sequence, closed reason, campaign head, and task-source digest. It stores no lead
summary or payload, creates no pod lease, and does not count as an accepted campaign unit.

## Restart Behavior

The campaign checkpoint is atomically replaced under the private campaign state directory. It binds
the original authority digest, repository identity, campaign worktree identity, branch, initial and
reconciled heads, active task, pending integration, completed count, typed blockers, deferrals,
satisfied-trigger guards, human-decision holds, and timestamps. Every prior checkpoint shape is
accepted only through an explicit migration that verifies the legacy bytes against their original
digest before supplying newer empty projections; a defaulted field never bypasses integrity
verification.

| Durable interruption point | Restart behavior |
| --- | --- |
| Before a unit starts | Revalidate the exact owned campaign worktree and continue planning. |
| After a unit is reviewed and before campaign integration | Reobserve the expected and target heads, apply or recognize the exact reviewed fast-forward once, then continue. |
| After a reconciled unit | Adopt the recorded head and do not replay the accepted unit. |
| After campaign completion | Return the identical completed outcome without starting a provider or Git effect. |
| During a correction provider session | Reload the exact run, worktree, candidate, correction round, and Claude session; reobserve any coordinator commit before resuming the same session. |
| During the initial implementation provider session | Preserve the state and stop; automatic initial-session recovery remains open. |

Content-free transient provider and session failures are retried as fresh isolated whole-unit
attempts, with a fixed maximum of three attempts. Exhaustion becomes a durable
`codingmage.campaign.provider_unavailable` pause. Quota and authentication failures are classified
separately and pause without consuming a transient retry attempt. These clean provider returns
release the owned unit before pausing, so the same exact campaign can continue after access is
restored. A process interruption during a correction keeps the active unit and resumes through
exact-session reobservation. An interruption during the initial implementation session remains
blocked from automatic replay.

Claude completion reports must select exactly one ready, blocked, or committed disposition. An
invalid or malformed report receives one resume of the same exact provider session with a
metadata-only correction instruction. If the resumed report is also invalid, the unit releases and
the campaign pauses with `codingmage.campaign.provider_invalid_output`; CodingMage does not retry the
whole unit or integrate its unaccepted candidate.

If deterministic gates or independent review still reject a unit after the configured correction
limit is reached, the one-unit coordinator returns a recoverable terminal state. The campaign
then clears its active-unit marker, releases the pod lease, retains the candidate branch for
inspection, and durably pauses with `codingmage.campaign.unit_recoverable_failure`. A terminal or
explicitly blocked one-unit outcome instead records `codingmage.campaign.unit_blocked`. A terminal
failure stops the campaign; an implementer-blocked task is durably excluded so independent work can
continue. No such path advances the campaign head or changes the active checkout.

Invalid or ambiguous lead ownership roots pause before implementation with
`codingmage.campaign.lead_invalid_owned_paths`. Once a unit starts, a repository inventory or
claimed-path mismatch blocks the campaign with `codingmage.campaign.unit_repository_boundary`.
Deterministic verification and provider failures pause with
`codingmage.campaign.unit_verification_failure` and `codingmage.campaign.unit_provider_failure`;
unexpected internal failures block with `codingmage.campaign.unit_internal_failure`. Each path
clears the durable active-unit marker, releases the pod lease, preserves any candidate branch or
scratch state needed for inspection, and never advances the campaign or active checkout.

A valid implementer-blocked disposition is a task result, not an internal provider failure. The
coordinator transitions that unit to `Blocked`, journals generic evidence without retaining the
provider's prose or blocker text, releases all owned resources, and adds the exact task ID to the
integrity-protected campaign checkpoint. Subsequent planning excludes that task while retaining it
as unchecked in the canonical roadmap. Independent ready work can continue; dependency descendants
remain unavailable through their normal unchecked dependency. When no unblocked ready task remains,
the campaign stops with `codingmage.campaign.no_unblocked_ready_work`. The configured `max_units`
ceiling counts both integrated units and durably blocked units so repeated blockers cannot produce
an unbounded provider-invocation loop.

## Current Limits

- Campaign checkpoints, accepted-head recovery, and exact correction-session recovery are
  implemented; complete campaign event journaling, attempt projections, and interrupted initial
  implementation-session resume remain open.
- Clean provider quota, authentication, and exhausted-transient pauses are durable and resumable;
  interrupted initial-implementation recovery and operator stop-after-unit controls remain open.
- Campaign execution is bounded by unit, attempt, correction, process, output, and resource limits;
  no monetary value is part of campaign or provider authority.
- Blocked tasks and deferrals survive restart, remain unchecked, and suppress only their exact task.
  Same-user blocker clearance and external-trigger observation are create-once and integrity-bound.
  A dedicated blocker-detail view and authenticated human-decision resolution remain open.
- Parallel live pods remain disabled even if the authority ceiling is greater than one.
- Story-level draft PR publication and authenticated GitHub campaign evidence remain open.
- A retained campaign branch requires human inspection; protected/default-branch promotion remains
  outside campaign authority.
- The prescribed ten-outcome disposable and ten-task controlled-target qualification gates remain
  open. Parallel pods and remote visibility cannot be enabled on the strength of the earlier pilots.
