# Decision 0011: Autonomous Roadmap Progression

- **Status:** Accepted
- **Date:** 2026-08-26
- **Decision owners:** Repository owner
- **Supersedes:** Informal assumptions that repeating bounded campaign units is sufficient for
  autonomous roadmap completion
- **Superseded by:** None

## Context

CodingMage safely executes bounded implementation and review units, but controlled-target evidence
showed that safe repetition alone does not produce useful unattended throughput. A lead can classify
an implementable task as outside authority, an oversized task can reach a provider without a usable
decomposition, and a campaign can stop after typed blockers without deterministically reconsidering
the remaining roadmap. These outcomes preserve the target, but they do not satisfy the product goal
of progressing through all locally implementable work without routine operator intervention.

## Decision

CodingMage will add one deterministic autonomous-progression layer above the existing task, pod,
review, integration, and recovery contracts. The layer must:

1. classify dependency readiness and external prerequisites before model planning;
2. bind every admitted unit to an exact authority envelope derived from canonical requirements;
3. decompose oversized canonical work only into sealed child packets whose aggregate authority,
   acceptance criteria, dependencies, and completion predicate equal the parent;
4. permit routine local engineering choices inside that envelope without granting architecture,
   external-effect, credential, protected-branch, or release authority;
5. defer genuine external blockers while continuing independent work;
6. create a new immutable planning generation after a completion, blocker, satisfied deferral,
   failed proposal, recoverable unit failure, or changed dependency observation;
7. route providers through deterministic risk and capability policy while preserving independent
   review strength;
8. recover stalled owned work through exact leases, processes, worktrees, checkpoints, and
   postconditions rather than name or process-pattern adoption;
9. integrate only into an isolated campaign branch and reconcile canonical plan, Git, tests,
   evidence, and durable state before claiming completion; and
10. expose content-minimized progress and stop reasons without prompts, source, provider prose,
    credentials, environment values, or hidden reasoning.

Automatic default-branch promotion, pull-request merge, release, signing, purchases, external
infrastructure changes, and owner-only decisions remain unavailable. A provider cannot create new
authority, mark an external prerequisite satisfied, or weaken a gate.

## Alternatives Considered

- **Continue one bounded task at a time:** safe but requires routine operator intervention and does
  not meet the autonomous-roadmap goal.
- **Give the lead unrestricted discretion:** rejected because model judgment cannot become
  repository, command, credential, integration, or completion authority.
- **Treat every blocker as terminal:** rejected because one unavailable task must not hide
  independent locally implementable work.
- **Merge directly to the destination branch:** rejected because campaign integration and product
  promotion remain separate effects.

## Consequences

- Planning becomes a durable deterministic service rather than a repeated prompt.
- Canonical task sources need enough dependency, path, acceptance, and completion structure to
  derive bounded authority. Ambiguous work remains a truthful human decision.
- Campaign completion requires reconciliation across all authoritative projections, not merely an
  empty ready set or a successful provider response.
- Qualification must demonstrate useful completion throughput as well as preservation and safe
  stopping behavior.

## Verification

- Mutation suites reject broadened, missing, duplicated, stale, and cross-task authority.
- Deterministic fixtures cover readiness, decomposition, blocker continuation, replanning,
  provider routing, watchdog recovery, integration refusal, and completion reconciliation.
- A frozen-clone campaign completes a supervised unit, a three-outcome pilot, and a bounded
  multi-task soak without changing the active source checkout or destination branch.
- External and unsupported work remains open and visible after local completion reconciliation.

