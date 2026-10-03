# ADR 0090: Admit Director Priority Through Mission Policy

- **Status:** Accepted for local implementation; exact-commit review pending
- **Date:** 2026-10-02
- **Decision owners:** CodingMage implementation

## Context

Decision 0088 introduced inert director input and priority packets. Decision
0089 closed their failure-code vocabulary. Task 33.1.1.2 requires a separate
admission boundary before a suggested order may influence scheduling. The
existing mission evaluator already owns delegated decisions, expiry,
revocation, budgets, risk, paths and gate tiers.

## Decision

`evaluate_director_priority` revalidates the complete input against fresh
campaign authority, owner charter, parsed task source, scheduler generation
and campaign head, then revalidates the proposal. The proposal must remain a
complete permutation of the scheduler-ready work. Every approved owner outcome
remains in the exact mission input; an `attention_criteria` index is only a
hint and never completion or authority. No new task, criterion or dependency
can be supplied by the proposal schema. Simultaneously ready tasks with an
explicit source dependency, or a ready follow-up alongside its source task,
are rejected as contradictory scheduler observations.

The coordinator constructs a closed `Ordering` decision from the verified
proposal digest. Its affected scope is the campaign's entire allowed path
set: before pod admission, a ready task may still use any of those paths.
The caller supplies a domain ID and alternative, which the existing mission
evaluator checks against the exact owner grant. Required gate tiers come
from that grant, and the decision carries routine risk because ordering
alone performs no repository effect. A narrower grant, a different decision
class, an unapproved alternative, expiry, revocation or exhausted decision
budget returns the evaluator's typed hold. Invalid policy observations fail
closed. Only `PermittedChoice` may be considered admitted by a future
coordinator consumer.

The function does not mutate the scheduler, admit a pod, satisfy an outcome,
write a decision record or call a provider. Task 33.1.1.3 owns bounded
director invocation and consumption of permitted suggestions. No new
dependency, licence, runtime adapter or model qualification is introduced.

## Verification

See `docs/evidence/sprint-33-director-priority-admission.md`. Focused tests
exercise a real reorder, no grant, unapproved alternative, wrong grant class
and scope, erased criterion, invented task, stale scheduler, conflicting
source dependency, expiry and revocation. Independent exact-commit review,
human-only acceptance and live qualification remain separate.
