# Unattended Safeguards

## Status and Scope

This document defines the approved target contract for unattended CodingMage campaigns. It is
normative policy, while [`TASKS.md`](../../TASKS.md) records implementation and evidence status.
Nothing here claims that an unchecked task or gate is implemented.

The first unattended release boundary is one local campaign, one implementation pod, and no remote
publication. Parallel pods, branch push, draft pull requests, default-branch merge, and release
publication each require later independent gates.

## Authority Invariants

- One campaign identity binds one repository identity, initial commit, task-source digest,
  operator-authorization digest, path policy, provider policy, gate policy, unit limit, protected
  branches, and publication mode.
- Repository text, task prose, provider output, diagnostics, issues, comments, and pull-request text
  are untrusted data and cannot alter authority.
- The active checkout must be clean at admission and must remain byte-for-byte outside CodingMage's
  mutation path.
- Implementers write only inside one coordinator-owned worktree and exact leased paths.
- Providers cannot invoke Git, unrestricted shell commands, network tools, MCP servers, credential
  helpers, merge operations, or release operations through repository instructions.
- Coordinator Git effects require exact expected identities, ancestry, paths, and postconditions.
- A task is complete only after implementation, applicable tests, deterministic gates, independent
  review, final verification, checkpointing, and exact task-source reconciliation succeed.

## Lead Dispositions

The lead returns one closed disposition. It cannot combine dispositions or attach executable
instructions.

| Disposition | Meaning | Required coordinator behavior |
| --- | --- | --- |
| `propose` | One dependency-ready task can run within current authority. | Validate snapshot, task, paths, risk, resources, artifacts, and gates before leasing. |
| `blocked` | A prerequisite is unavailable and cannot be repaired inside current authority. | Persist the typed blocker, leave the task unchecked, suppress reselection, and continue independent work. |
| `deferred` | A validated temporary scheduling condition prevents safe admission now. | Persist the reason and exact reconsideration trigger, release no work, and revisit only after that trigger. |
| `human_decision_required` | Scope, architecture, authority, or external consequences require the owner. | Record a content-free decision request and continue only work independent of that decision. |

An invalid response is not a disposition. Unknown fields, mixed outcomes, stale identities,
duplicate tasks, escaping paths, undeclared artifacts, contradictory dependencies, and unsupported
reason codes are rejected before any lease or provider invocation.

## Reason Classes

Blocked reasons are limited to unavailable external dependency, unavailable supported hardware,
missing operator-managed authentication, unavailable external service, unsupported platform,
blocked prerequisite, or an implementation-discovered condition that cannot be resolved inside the
leased task and paths.

Deferred reasons are limited to temporary provider capacity, active path lease, gate-resource
contention, deterministic dependency ordering, pending stronger review, or an accepted operator
pause. Every deferral names one trigger: campaign-head advancement, lease release, gate-resource
release, provider reset, review completion, or operator resume.

Human-decision reasons are limited to ambiguous scope, material architecture choice, requested
authority expansion, protected-branch consequence, external infrastructure change, or release
decision.

The same task cannot be deferred again against the same campaign head and already-satisfied trigger.
That repetition becomes a precise no-progress condition requiring a new observable trigger or human
decision. This prevents starvation and model-generated defer loops.

## Independent Limits

The coordinator enforces separate ceilings for:

- Accepted campaign outcomes.
- Simultaneous pods.
- Whole-unit provider attempts.
- Same-session malformed-report repair.
- Gate and review correction rounds.
- Process descendants and open files.
- CPU time, memory, output bytes, artifact bytes, and retained state.
- Gate duration, provider duration, and total campaign elapsed execution.

Exhausting one ceiling stops or pauses under its own typed reason. The coordinator cannot compensate
by increasing another ceiling, selecting a weaker model, skipping a gate, truncating required
evidence, or changing publication policy.

## Repository and Git Safety

1. Inventory and authorize the exact clean active checkout.
2. Refuse unsafe ownership, in-progress Git operations, linked authority files, hostile repository
   execution features, replacement refs, and unapproved submodule or large-file behavior.
3. Create a collision-resistant coordinator branch and worktree from the exact authorized commit.
4. Revalidate filesystem and Git identity before every mutation.
5. Inventory the complete candidate diff and reject every unleased path.
6. Commit through fixed coordinator commands with hooks, aliases, editors, signers, filters,
   credential helpers, pagers, and ambient Git environment disabled.
7. Integrate only an exact reviewed descendant through compare-and-swap campaign-head advancement.
8. Never force push, rewrite history, reset user work, delete an unowned branch, merge a protected
   branch, or remove a dirty or identity-mismatched worktree.

## Credentials, Network, and Provider Identity

- Configuration stores policy and references, never credential values.
- Existing provider login discovery receives only the documented minimal environment allowlist.
- Provider version, capability surface, requested model, and resolved model are checked and recorded
  when available.
- An unavailable required profile stops; CodingMage does not silently fall back across provider or
  strength boundaries.
- Implementation sandboxes deny repository-requested network, browser, MCP, skill, subagent, and
  credential access.
- GitHub access, when later enabled, uses an authenticated CLI outside model control and exact
  repository, account, host, branch, and capability grants.

## Durable Recovery and Cleanup

- Journal intent before each provider, process, commit, integration, publication, and cleanup effect.
- Journal observation after each effect using exact identities and content-minimized evidence.
- Resume only idempotent or positively reobserved phases.
- Preserve uncertain candidates, branches, worktrees, and journals for diagnosis.
- Terminate and remove only exact owned processes and artifacts.
- Never adopt a similarly named concurrent process, branch, worktree, or service unit.
- Preserve staged, unstaged, untracked, stashed, noted, configured, and concurrently created user
  state outside the owned worktree.

## Operator Controls and Observability

The operator must be able to inspect status, explain a blocker, open the retained diff or sanitized
log, pause, resume, stop after the current unit, and cancel. Mutating controls require same-user and
exact-run authorization plus an idempotency key.

Status includes campaign and task identity, phase, actor, model identity when available, attempt,
correction count, completed count, blocked count, deferred count, current limit utilization,
elapsed execution, and content-free reason codes. It excludes prompts, source text, filenames,
provider prose, command output, unrestricted environment values, credentials, and hidden reasoning.

## Ten-Outcome Disposable Soak

The deterministic disposable soak uses one pod, local-only publication, and exactly ten accepted
outcomes. The fixture schedule must include:

1. A clean implementation and review pass.
2. A recoverable gate failure followed by a passing correction.
3. A review finding followed by a passing correction and fresh full-diff review.
4. A truthful external blocker while independent work continues.
5. A temporary deferral that becomes eligible only after its recorded trigger.
6. A malformed lead or provider report that is rejected or repaired within its exact limit.
7. A provider-capacity pause and validated resume.
8. An interruption after durable intent and recovery without duplicate effect.
9. A stop-after-unit control that ends at a clean checkpoint.
10. A final accepted unit followed by exact ceiling enforcement.

The soak fails on any false completion, duplicate task, skipped gate, unreviewed commit, unauthorized
path, active-checkout mutation, orphan process, leaked worktree, unbounded retained state, silent
model downgrade, or unverifiable evidence record.

## Ten-Task Controlled-Target Soak

After the disposable soak passes, CodingMage may run ten dependency-ready tasks on a dedicated clean
target branch with one pod and local-only publication. The campaign may encounter blockers or
deferrals, but the evidence report must distinguish them from completed tasks and cannot claim ten
completed tasks unless ten tasks actually pass every completion gate.

Before execution, capture the target identity, clean status, branch, head, task-source digest,
configuration digest, provider capabilities, gate registry, scratch and state roots, process
baseline, and operator authorization. After execution, reconcile every selected task, disposition,
commit, correction, gate, review, checkpoint, process, worktree, branch, task checkbox, and active
checkout byte-for-byte where applicable.

## Release and Publication Gates

The first public binary release is Linux-only unless native evidence expands that claim. Publication
requires all of the following:

- Every locally implementable task and gate required for the release is complete with immutable
  evidence.
- Complete workspace unit, integration, CLI workflow, mutation, adversarial, recovery, process,
  Git, documentation, packaging, installation, upgrade, rollback, and removal tests pass.
- Disposable and controlled-target soak reports pass after the last reliability change.
- Dependency provenance, licenses, SBOM, checksums, build manifest, source archive, release notes,
  supported-platform statement, unsupported-behavior statement, and security-reporting path are
  complete.
- Two clean release builds produce the expected reproducible identity.
- Manual fuzzing and independent human security and architecture review are reconciled truthfully.
- A signed release candidate is installed and exercised from its packaged artifact in a clean user
  environment.
- The repository owner explicitly approves default-branch merge, signed tag creation, and release
  publication as separate actions.
- Published assets, checksums, signatures, SBOM, installation instructions, and version output are
  downloaded and verified independently after publication.

No model may self-approve, merge, tag, publish, alter repository settings, manage secrets, or delete
the release branch. A failed post-publication verification triggers documented disablement or
withdrawal without rewriting release history.

## Required Test Families

| Family | Minimum proof |
| --- | --- |
| Unit and schema | Every enum, parser, validator, transition, limit boundary, unknown field, and canonical serialization path. |
| Property and mutation | Every authority, journal, checkpoint, evidence, lease, and provider-report field fails safely when changed. |
| Integration | Fake lead, implementer, reviewer, gates, Git, process, monitor, service, and publication adapters execute complete workflows. |
| Git and filesystem | Dirty state, concurrent edits, hostile configuration, identity replacement, collisions, interruption, and cleanup preserve user state. |
| Process and resource | Timeout, cancellation, descendant escape, output pressure, process pressure, sleep, logout, restart, and shutdown remain bounded. |
| Provider | Authentication, quota, overload, malformed output, unavailable profile, changed model identity, correction, and resume follow exact policy. |
| Campaign | Dependencies, blockers, deferrals, starvation, retries, corrections, restart, stopping, and outcome ceilings remain deterministic. |
| Publication | Push and draft-PR idempotency, concurrent human edits, permission loss, timeout reconciliation, protected branches, and prohibited operations. |
| Packaging | Reproducible build, provenance, SBOM, checksums, install, verify, upgrade, rollback, remove, and retained-data policy. |
| Soak | Prescribed ten-outcome disposable campaign and ten-task controlled-target campaign pass with reconciled residue. |
