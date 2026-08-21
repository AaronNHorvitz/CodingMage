# Decision 0008: Unattended Safeguard and Release Boundary

- **Status:** Accepted
- **Date:** 2026-08-21
- **Decision owners:** Repository owner
- **Supersedes:** Informal unattended-campaign stopping and publication assumptions
- **Superseded by:** None

## Context

CodingMage can execute bounded supervised units and an initial serial campaign, but completing a
large roadmap unattended requires more than repeating successful units. The coordinator must
distinguish permanent blockers from temporary deferrals, prevent repeated model output from
creating an infinite queue, preserve exact repository and process ownership, expose operator
controls, and establish evidence before any remote publication authority is enabled.

The release plan also needs an exact final boundary. A passing model review does not authorize a
merge or release, and a successful local campaign does not prove packaging, installation,
recovery, or public artifact integrity.

## Decision

1. Campaign authority is immutable for one campaign identity. Repository identity, starting
   commit, task-source digest, allowed and denied paths, provider profiles, gate tiers, unit limit,
   protected branches, and publication mode are bound before a provider starts.
2. The read-only lead may return exactly one of `propose`, `blocked`, `deferred`, or
   `human_decision_required`. Lead output remains untrusted until the coordinator validates its
   snapshot, reason code, task identity, dependencies, and authority.
3. `blocked` records an unavailable prerequisite, leaves the canonical task unchecked, suppresses
   repeated selection, and leaves descendants unavailable through normal dependency rules.
4. `deferred` records a temporary validated scheduling condition and an exact reconsideration
   trigger. It cannot complete a task, permanently hide work, or repeat against an unchanged
   snapshot and trigger indefinitely.
5. Invalid, stale, contradictory, mixed, or unauthorized lead output is rejected without creating
   a lease, worktree, provider invocation, Git effect, or task-state transition.
6. Initial unattended execution uses one pod, local-only publication, and an exact ten-outcome
   ceiling. Concurrency and remote visibility remain separate later gates.
7. Provider attempts, malformed-response retries, corrections, processes, output, storage, elapsed
   execution, and accepted task outcomes are independently bounded. Reaching one limit cannot
   silently weaken another.
8. Models receive no direct Git, credential, protected-branch, merge, release, repository-setting,
   or external-infrastructure authority. Coordinator adapters alone may perform explicitly granted
   and independently verified effects.
9. Every state-changing intent is journaled before execution and reobserved afterward. An uncertain
   effect is never replayed merely to discover whether it happened.
10. Public release requires a separate owner decision after the complete local test matrix,
    adversarial corpus, ten-outcome disposable soak, ten-task controlled-target soak, packaging,
    installation, recovery, provenance, independent review, and deferred manual fuzz gate are
    truthfully reconciled.
11. Initial publication may push only an exact verified feature or release-candidate branch and
    create a draft pull request. Default-branch merge, signed tag creation, and release publication
    remain explicit human actions.

## Consequences

- Lead and campaign schemas require closed disposition and reason-code enums.
- Durable state must project blocked and deferred sets, reconsideration triggers, attempts,
  corrections, operator controls, and accepted outcomes independently.
- The scheduler must prove starvation resistance and deterministic reconsideration.
- A ten-outcome ceiling is a safety limit, not evidence by itself; the prescribed fixture and
  controlled-target acceptance conditions must also pass.
- One-pod soak evidence precedes parallel-pod enablement.
- Remote publication work cannot be used to bypass incomplete local evidence.
- Unsupported platform evidence remains explicit and does not prevent a truthfully scoped Linux
  release.

## Verification

- Mutate every campaign-authority field and prove validation fails before provider or Git effects.
- Exercise every lead disposition, reason code, mixed response, stale snapshot, duplicate response,
  and reconsideration trigger.
- Interrupt every journaled intent and prove restart reobserves or blocks without replay.
- Run the exact ten-outcome disposable campaign with prescribed completion, correction, blocker,
  deferral, interruption, and cancellation cases.
- Run a local-only ten-task controlled-target campaign from a dedicated clean branch and reconcile
  every task, commit, gate, review, process, worktree, and active-checkout invariant.
- Build the release candidate twice, verify artifact identity and provenance, install and remove it
  from a clean user environment, and verify the public artifact after owner-authorized publication.
