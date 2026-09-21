# Owner-Optional Team Planning Handoff

## Scope and Source

This is a documentation verification and implementation handoff dated 2026-09-21, not product
qualification or independent review. The planning worktree starts from
`4390b02a5ecc8b2d56388aa3daa666742802e190` on `plan/autonomous-engineering-team`, independently
of the Muse implementation and its pinned review branch. Do not replace either branch's work or
infer that this planning branch includes later implementation/reviewer corrections.

The owner requested director/team-lead/pod/reviewer/QA coordination and the option to remain
completely hands-off after authorizing the campaign. The accepted planned contract is recorded in
[Decision 0014](../decisions/0014-owner-optional-engineering-team.md),
[PRD.md](../../PRD.md), [the development plan](../../PRODUCT-DEVELOPMENT-PLAN.md),
the [team architecture](../architecture/autonomous-engineering-team.md) and Sprints 32 through 35
of [TASKS.md](../../TASKS.md). README, existing architecture, operational and contribution/security
guidance distinguish planned modes from current execution and preserve separate delivery authority.

## Verification

The unchanged source baseline and updated documentation working tree each ran the full Python
suite under `MemoryHigh=5G`, `MemoryMax=6G`, `MemorySwapMax=512M`, with more than 16 GiB available
RAM. No Rust source, provider, dependency, runtime schema or toolchain changed.

| Check | Baseline | Updated documentation |
| --- | --- | --- |
| `python3 -m unittest discover -s tests -p 'test_*.py'` | 38 tests, 37 pass, one failure | 38 tests, 37 pass, one failure |
| Evidence freshness failure | `crates/codingmage-campaign/src/team.rs`, `README.md` input drift | Same two inputs plus intentionally edited `SECURITY.md` input drift |
| `python3 scripts/docs_check.py` | Not separately rerun before editing | Pass |
| `python3 scripts/check_architecture.py` | Not separately rerun before editing | Pass |
| `git diff --check` | Clean initial worktree | Pass |
| Planning traceability inspection | Original task identifiers/statuses captured | 988 existing task/AC/gate checkbox identities and states unchanged; 16 PRD requirements map to 16 new tasks |
| New task/dependency inspection | No Sprints 32 through 35 | 64 sub-tasks, eight ACs and eight gates all unchecked; 63 declared dependency rows reference existing identifiers without a cycle |

The failing test is
`test_multi_agent_matrix.MultiAgentScenarioMatrixTests.test_multi_agent_evidence_binding_is_current`.
Its pre-existing failure is not fixed here, and the extra security-document drift is a known
consequence of this scope. No retained binding hash, input set or historical report was changed to
conceal either result. Task 25.2.4.6 still requires legitimate execution/review-backed renewal.
The successful checks above validate documentation/architecture consistency, not the new behavior.

## Implementation Handoff

Begin the owner-optional workstream with Sub-task 32.1.1.1 after reconciling the intended integration
base and the independent implementation review. Preserve prior stable task identities and honest
status changes from that review. Do not assume a simple branch merge itself qualifies either set.

Reconcile exact existing prerequisites, then implement dependency-ready contracts and fakes through
Sprints 32 through 34. Preserve the existing scheduler, journals, reviewer separation and effect
boundaries. Only after exact current-source and live prerequisites exist may Sprint 35 claim a
no-intervention campaign, first with one pod and then separately qualified higher capacities.

The initial milestone is local-only. Optional remote delivery is separately granted and tested.
Human-required reviews, releases, signing, purchases, credentials and infrastructure effects are
not delegated by selecting hands-off. All 96 new task/sub-task/AC/gate checkboxes remain open.

No active agent has been redirected, stopped or restarted. No implementation worktree has been
edited by this planning increment. It contains no authorization to push, merge or publish, and no
runtime test, release, live-provider or independent-review claim.
