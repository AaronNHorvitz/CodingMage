# Decision 0014: Owner-Optional Autonomous Engineering Teams

- **Status:** Accepted for planning and staged implementation
- **Date:** 2026-09-21
- **Decision owners:** Repository owner
- **Supersedes:** Decision 0011's exclusion of all architectural decisions from routine delegation,
  only for explicitly bounded decision domains in the new planned mission contract; implicit
  assumptions that the product owner must answer routine questions during a campaign
- **Superseded by:** None

## Context

The owner requested an engineering-team workflow with director, team lead, implementation pods,
independent review, QA, integration and feedback, plus the option to be completely hands-off after
initial authorization. Existing hierarchical campaigns and autonomous progression already provide
substantial foundations. More manager prompts or unrestricted provider processes would not supply
the missing product contract or reliable end-to-end qualification.

## Decision

1. Adopt [PRD.md](../../PRD.md), [the development plan](../../PRODUCT-DEVELOPMENT-PLAN.md),
   [the team contract](../architecture/autonomous-engineering-team.md) and unchecked Sprints
   32 through 35 as the planned extension. Reuse the existing coordinator and canonical journal.
2. Make supervised, exception-only and hands-off owner involvement explicit, independently of
   pod concurrency, provider selection and publication/promotion policy. Old configurations do not
   opt in. A mode selector alone creates no execution or external-effect authority.
3. Admit one immutable authenticated mission charter before work. It binds outcomes, exclusions,
   permitted decision domains, policy, exact target identities, independent limits and expiry.
   Routine architecture, dependency and interface choices may be delegated inside named domains;
   agents cannot redefine the mission, admission policy, review rules or prohibited effects.
4. In hands-off mode, issue no mid-campaign interactive question or approval. Apply only a
   preauthorized disposition, retain otherwise-blocked work, and continue independent eligible
   tasks. Never simulate owner responses, treat silence as consent or create credentials.
5. Model-backed directors and leads remain proposal producers. Only the coordinator may authorize
   effects, update canonical task state, create commits, run gates and apply integration policy.
   QA and specialist reviewers are bounded and independent of implementation contributions.
6. Extend existing scheduling with declared interface/semantic dependencies, product acceptance,
   source-backed handoffs and useful-outcome feedback. No new execution loop, competing state store,
   mandatory memory service or dependence on another repository is introduced.
7. Retain automatic integration into an isolated campaign branch as a separately granted effect.
   Feature publication and eligible automatic destination promotion require their own explicit
   policy, exact effect binding and qualification; hands-off mode does not enable them.
8. Preserve human-only reviews, protected release gates, signing, purchases, credentials, deployment,
   infrastructure and prohibited destructive effects. This planning approval grants none of them
   and does not authorize any current agent to push, change its scope or resume stopped work.
9. Permit dependency-ready local implementation/fakes for the new sprints after exact prerequisites
   and earlier blockers are reconciled. Preserve existing live qualification and staged-concurrency
   gates. Optional remote delivery is not a prerequisite for local hands-off operation.

## Consequences

The product owner need not operate the team during an admitted campaign. Authority remains finite,
revocable and auditable. No-intervention execution is not a guarantee that every objective is
achievable; unavailable required evidence produces an incomplete outcome with exact reasons.
Planned policy-driven destination promotion must not be confused with a currently qualified live
capability or authority over CodingMage's own release process.

Director and team-lead roles may initially share a qualified planning profile but have separately
validated packets and lifetimes. Reviewer independence follows actual authorship and source-bound
evidence, not provider branding. An agent correcting a defect becomes an implementer for that diff.

## Alternatives Considered

- One unbounded conversation cannot provide durable ownership, exact review or crash reconciliation.
- A prompt-only manager hierarchy does not enforce permissions, resource reservations or Git state.
- Human approval for every engineering choice defeats the requested hands-off product mode.
- Globally bypassing permissions would expand effects rather than delegate defined decisions.

## Verification and Rollout

Require the PRD's mode matrix, authority mutations, disconnect/expiry/revocation, independent review,
cross-pod interfaces, realistic product acceptance, no-progress handling and crash reconciliation.
Qualify a real one-pod no-intervention campaign before increasing live concurrency. Preserve every
failed attempt and exact supported role/provider/platform claim. All new implementation gates remain
open. Acceptance of this document neither refreshes historical evidence nor satisfies human review.
