# Decision 0005: Retain The Independent Coordinator

## Status

Accepted for the current bootstrap and pilot lifecycle.

## Context

CodingMage was created to coordinate bounded implementation and senior-review loops across independently authorized repositories. It has a narrow product boundary and locally executable repository, process, provider-adapter, gate, orchestration, recovery, monitoring, service, GitHub, adversarial, packaging, CLI, and project-isolation components.

Making CodingMage depend on a target repository's implementation would replace that narrow boundary with an unreviewed transitive dependency and could make either project capable of invalidating the other's development path.

## Decision

CodingMage remains an independent repository, process, state root, worktree root, configuration, credential-reference namespace, and release lifecycle through its disposable soak and controlled-target pilots.

CodingMage may coordinate an authorized target, but it must not import target authority policy dynamically, rewrite itself, share writable runtime state, or claim product support that has not been verified.

Retirement or conversion into a thin client requires a new decision after all of these conditions are met:

1. A replacement exposes one supported, versioned coordinator interface matching CodingMage's required task, repository, process, gate, review, recovery, monitoring, and publication semantics.
2. Pairwise behavior and adversarial tests show equal or stronger fail-closed authority.
3. A deterministic migration maps every active run, task, worktree, session, evidence, blocker, and operator control without replaying state-changing effects.
4. Parallel shadow operation and rollback pass on disposable repositories.
5. Independent review finds no unacceptable regression.
6. The product owner explicitly approves migration and later archival.

## Alternatives

- **Retire immediately:** rejected because no replacement has passed the required compatibility and migration gates.
- **Make CodingMage a target-owned module:** rejected because it creates circular bootstrap and authority coupling.
- **Keep both permanently without review:** rejected because unnecessary duplication should be reevaluated after comparable mature behavior exists.

## Consequences

- CodingMage can continue validating the narrow two-agent development loop independently.
- Target repositories can evolve without becoming unreviewed transitive authority sources.
- Some mechanisms may be temporarily duplicated.
- Future migration requires explicit compatibility, shadow, rollback, review, and owner gates.

## Migration And Archive Procedure

If a later decision approves migration, freeze new CodingMage runs, complete or block active units, export content-minimized state through a versioned converter, import into an isolated replacement candidate, compare exact projections, execute shadow workflows, test rollback, and retain the CodingMage source and signed final manifest as a read-only archive. Do not delete historical evidence, rewrite published history, or silently redirect installed clients.

## Supersession

No prior decision is superseded. A future decision must name this record explicitly.
