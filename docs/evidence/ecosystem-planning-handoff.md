# Ecosystem Planning And Development Handoff

- **Report date:** 2026-09-20
- **Repository:** CodingMage only
- **Destination branch:** `feat/hierarchical-campaigns`
- **Starting commit:** `37e7e51ddec36adecc759f11a28e00e8e8f9487a`
- **Scope:** Owner-requested planning reconciliation and branch push, before external development
  with Muse Code. No integration implementation or live qualification is claimed.

## Planning Changes

Decision 0012 now records acceptance on the date of the owner's reconciliation request; acceptance
is not backdated and is not independent review. Decision 0013 admits optional host, USTE context
and Muse provider work while retaining the independent coordinator, journal and first-release order.
The integration requirements and Sprints 29 through 31 define the future contracts, separate
dependencies and consumer-side acceptance cases. Every new implementation and qualification box
remains open.

The README distinguishes the implemented Claude/Codex roles from future provider choice. Operations
guides list the three integrations as unimplemented, with no qualified versions. The host uses a
role name in public documentation to preserve Sub-task 25.2.5.44's downstream-name privacy rule.
No sibling checkout, process, branch, runtime state or model configuration was changed.

## Baseline Verification And Evidence Debt

The unchanged starting commit ran:

```bash
python3 -m unittest discover -s tests -p 'test_*.py'
```

Result: **38 tests, 37 passed, 1 failed**, exit 1, 32.041 seconds. The only failure was
`test_multi_agent_evidence_binding_is_current`, reporting exactly:

```text
input-drift:crates/codingmage-campaign/src/team.rs
input-drift:README.md
```

The retained `docs/evidence/multi-agent-evidence-binding.json` binds source
`319de727917928057c04d7da0a975706483623ef`. Later source and documentation changes predate this
planning batch. This batch also changes the README; the old binding cannot qualify its result.
The binding, input set, historical commands, package digests and review claims remain unchanged.
Task 25.2.4.6 was added, and Task 25.2.4, AC 25.4 and Gates 25.3/25.4 were reopened to represent
current verification accurately. This is not a passing release or full-suite report.

The baseline Python process was observed inside `codingmage-plan-baseline.scope`, with
MemoryHigh=5368709120, MemoryMax=6442450944 and MemorySwapMax=536870912 bytes. One Cargo job and one
test thread were configured. No Cargo build or live provider was started for this planning batch.

## Verification Of The Planning Batch

| Check | Result |
| --- | --- |
| `python3 scripts/docs_check.py` | Passed: document policy, links, diagrams and configured secret checks |
| `python3 scripts/check_architecture.py` | Passed: existing dependency direction preserved |
| `python3 -m unittest discover -s tests -p 'test_*.py'` | 38 tests in 34.019 seconds; 37 passed and the same one baseline freshness failure, exit 1 |
| Task-structure inspection | 656 unique sub-task IDs, 26 explicit dependency entries resolving without cycles, and all 46 new integration checklist rows unchecked |
| Existing downstream-name privacy test | Passed as part of the full suite with all new documents staged |
| `git diff --check` and `git diff --cached --check` | Passed |
| Runtime/test/evidence-input preservation | No Rust, test or script edits; retained multi-agent binding unchanged |

The updated suite was observed inside `codingmage-plan-verification.scope` with the same
5368709120/6442450944/536870912-byte memory limits as the baseline. Available memory before the
run was approximately 48 GiB, with no swap in use. No new Python failure was introduced. The full
suite is still failing and must not be reported as green. No Rust build, package regeneration,
live integration, native-platform or independent-review result is claimed for this increment.

Review of this documentation diff is an agent self-review, not the independent human review
required for release. Branch publication is authorized by the owner's planning request and does
not promote or qualify a release candidate.

## Exact Continuation For Muse Or Another Development Tool

1. Read README, TASKS, CONTRIBUTING, Decisions 0002 through 0005 and 0011 through 0013, and the
   [integration requirements](../architecture/ecosystem-integration.md). Reinspect actual branch,
   clean/dirty state, other writers, installed capabilities and RAM/swap after any restart.
2. Use an isolated CodingMage worktree. Do not run CodingMage against its own source or adopt an
   old campaign's state. Preserve the active host and USTE development sessions and checkouts.
3. Start with Sub-task 25.2.4.6: review exact drift against the retained binding, inventory all
   affected evidence and construction/review prerequisites, and perform permitted renewal as a
   single batch after related source work. Do not manufacture a review record or merely recompute
   input hashes. Record unavailable prerequisites exactly and continue other dependency-ready work.
4. Follow the existing first-release dependency order, including remaining native Windows work.
   Only if earlier work is actually blocked may the explicit local-preparation exception select
   Story 29.1, then independent USTE or Muse preparation. No Muse runtime adapter is needed to use
   Muse as a development tool.
5. Use one Cargo build job, one test thread and one heavy workload per session; check combined
   host headroom and apply MemoryHigh=5G, MemoryMax=6G and MemorySwapMax=512M systemd user scopes
   where supported, with actual child-cgroup verification. Honor stricter limits and preserve
   unrelated processes. Do not solve resource failures by removing limits or lowering gates.
6. Keep native qualification, authenticated services, independent human review, signing and release
   publication open until their real prerequisites and exact evidence exist. Cross-product work
   additionally needs counterpart admission and a pinned tested interface. Push only reviewed,
   authorized feature-branch increments; no force push or default-branch merge.

## Open Gates

Current-source evidence renewal; the remaining standalone frozen-target and native-platform gates;
authenticated GitHub qualification; manual fuzzing; independent human review; signing; explicit
release authorization; and all implementation/qualification in Sprints 29 through 31 remain open.
Documentation and acceptance of the planning boundaries close none of those gates.
