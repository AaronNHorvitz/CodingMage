# Decision 0007: Hierarchical Campaign Pods

- **Status:** Accepted
- **Date:** 2026-08-20
- **Decision owners:** Repository owner
- **Supersedes:** Single-unit-only progression as the intended final operating model
- **Superseded by:** None

## Context

The first authenticated target run proved that the supervised one-unit boundary protects the target
repository, but it also proved that stopping after the first deterministic gate failure cannot
complete a large roadmap unattended. Large plans contain both sequential dependencies and
independent work. They need bounded correction, durable continuation, independent review, and
carefully limited concurrency without allowing models to grant themselves broader repository or
publication authority.

Creating one GitHub pull request per sub-task would add remote churn, consume CI capacity, and make
GitHub metadata compete with the canonical local task plan. Conversely, allowing every agent to
merge directly would erase independent review and make conflict handling nondeterministic.

## Decision

CodingMage will use a hierarchical campaign model:

1. A deterministic campaign coordinator reparses the canonical task source and owns all state
   transitions.
2. A read-only campaign lead may propose dependency-ready pod assignments, path leases, risk
   classifications, and verification depth. Its output remains untrusted until deterministic
   validation.
3. Each pod receives one exact task, one isolated worktree, nonoverlapping path authority, an
   implementation agent, deterministic gates, and an independent review agent.
4. Gate failures and accepted review findings return to the pod implementer through one shared,
   bounded correction limit. Every correction creates a new coordinator-owned commit, reruns local
   gates, and receives a fresh review of the complete cumulative task diff.
5. A deterministic integration lead serializes Git mutations, verifies exact ancestry and path
   ownership, composes accepted pod commits into a campaign branch, and runs batch-level gates.
6. GitHub publication is optional and subordinate to local authority. The default remote view is
   one issue and one draft pull request per coherent story or integration batch.
7. Pod agents and review agents never push, merge, approve, release, alter repository settings, or
   write protected branches. Only coordinator adapters may perform explicitly configured effects.
8. Autonomous promotion may advance only the configured campaign branch. The default branch remains
   protected and requires a separately configured final promotion policy.
9. Initial rollout uses one pod. Concurrency increases only after serial correction, continuation,
   recovery, and integration soak evidence passes.

## Alternatives Considered

- **One autonomous agent:** rejected because implementation and approval would share one failure
  domain and one context.
- **One PR per sub-task:** rejected as the default because it creates excessive remote and CI churn.
- **Let every pod merge:** rejected because overlapping decisions and nondeterministic order would
  weaken review and recovery.
- **Unlimited parallel pods:** rejected because repository paths, tests, provider quotas, CPU,
  memory, and integration order are bounded shared resources.
- **Keep the current one-unit stop behavior:** rejected because it requires manual intervention for
  ordinary compilation and review corrections.

## Consequences

- Campaign execution requires explicit campaign-level path and resource authority in addition to the
  existing per-unit authority.
- The repository-wide lock must evolve into one campaign lease plus exact task/path leases and a
  serialized Git mutation queue.
- Model usage rises with pod count, but deterministic pre-review gates and risk-based routing remain
  available to bound provider activity.
- The scheduler must detect dependency, path, test-resource, and integration conflicts before
  starting pods.
- Story-level PRs provide human visibility without becoming the canonical source of completion.
- Protected/default-branch merge remains unavailable until a separate policy and evidence set is
  explicitly approved.

## Verification

- A gate-failure fixture must correct and pass without operator intervention.
- A review-finding fixture must correct, rerun gates, and receive independent rereview.
- A campaign fixture must advance multiple dependency-ordered tasks from an evolving campaign head.
- Parallel fixtures must prove that overlapping path or resource leases cannot run concurrently.
- Integration fixtures must permute pod completion order and produce one deterministic campaign
  result or a precise conflict.
- Crash, quota, cancellation, stale-plan, stale-head, and concurrent-user-change fixtures must
  resume or stop without duplicate effects.
- GitHub fixtures must prove story-level draft PR idempotency and complete absence of model-owned
  merge authority.
