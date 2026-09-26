# CM-R03 Reproducible Repair and Baseline-Bound Receipts - Local Evidence

- **Status:** Local implementation with deterministic tests only; no live provider or
  independent review is claimed.
- **Work package:** CM-R03 (CAP-10, 11, 14, 15, 31, 38), split in `TASKS.md`
- **Source revision:** the commit that carries this document

## CM-R03.1 - Baseline-bound gate receipts

Implemented in `crates/codingmage-runtime/src/gate_baseline.rs` and the workflow port's
`compare_with_baseline`:

- A candidate gate run that passes every configured gate is retained as the `GateBaseline` of
  its own commit and gate-registry digest under `state_root/gate-baselines/<repository_id>/`
  as an integrity-protected document. No extra process is ever run to obtain a baseline, so
  unit process counts and budgets are unchanged.
- Every candidate gate run is classified against the baseline of its base commit and retained as
  a `GateComparison`: `introduced` (failed now, passed at base), `pre_existing` (failed at both),
  `repaired` (passed now, failed at base) and `unclassified` (failed now, no baseline outcome).
  A base commit without a retained baseline yields `baseline = "unknown"`, which is an honest
  gap and never a pass. The comparison's integrity digest is appended to the unit's gate
  evidence, so the durable receipt binds the classification to the exact base and candidate.
- Behavior is otherwise unchanged: a failing required gate still fails the unit whether the
  failure is new or pre-existing. Refusing a unit whose base already fails a required gate, and
  an operator command that observes the baseline of a fresh head explicitly, are recorded as
  open under CM-R03.2.

Tests: `cargo test -p codingmage-runtime gate_baseline` (classification, unknown baseline,
identity and tamper refusal, store round trip); the supervised workflow test keeps its exact
process and utilization counts, proving no hidden gate run was added. The native workspace
now shows three gate evidence records per accepted unit with two configured gates: the two
gate receipts and the baseline comparison receipt (`codingmage-ui` changes test).

## CM-R03.2 - Reproduce-before-repair contract

Implemented in the supervised run spec (`RunSpec.repair`, `RepairRequirement`) and the workflow
port (`reproduce_regression`, `complete_repair_receipt`) with `RepairReceipt` in
`gate_baseline.rs`:

- Before the implementer runs, the coordinator's gate runner executes only the named configured
  gate at the base commit inside the unit's worktree. A passing gate refuses the unit with
  `codingmage.runtime.repair_not_reproduced` before any provider process; in a campaign that unit
  error is a typed blocker (`codingmage.campaign.unit_repair_not_reproduced`). An unknown gate
  identity or malformed name is a spec refusal.
- After the candidate's gates run, the named gate must pass; the coordinator then retains an
  integrity-protected `RepairReceipt` binding the failing base observation digest and the passing
  candidate observation digest, and appends the receipt digest to the unit's gate evidence. A
  still-failing regression gate follows the ordinary bounded correction path with no receipt.
- Ordinary run specs without `[repair]` are byte-for-byte unchanged in behavior and process counts.
- Not done: carrying the requirement through campaign task authority (`task_path_authority`);
  campaigns still run repair units only through the supervised run spec.

Tests: `cargo test -p codingmage-runtime gate_baseline` (receipt requires a failing base and a
passing candidate, refuses other gates and tampering) and `cargo test -p codingmage-cli --test
repair` (real coordinator process: reproduced regression repaired and receipted with exact base
and candidate commits; a gate that already passes is refused before any provider runs and leaves
the active checkout untouched; unknown or escaping gate names are spec refusals; an ordinary unit
is unchanged).

## CM-R03.3 - Crosswalk of exact-commit review and bounded repair against CAP-15 and CAP-38

| Acceptance statement | Implementation | Tests and evidence | Status |
| --- | --- | --- | --- |
| CAP-15: bounded correction attempts linked to the exact finding and commit | Sprint 11 finding ledger and correction packets (`codingmage-review`), Sprint 20 durable correction checkpoints, Sprint 26 correction timeout ledger | `docs/evidence/sprint-11.md`, `sprint-20-correction-recovery.md`, `sprint-26-correction-timeout-hardening.md`, `sprint-26-supervised-recovery-attempt-ledger.md` | Local deterministic evidence; live correction is Gate 20.2 (open) |
| CAP-15: deduplicate PR and comments | Sub-tasks 24.2.1.2, 24.2.1.4, 24.2.1.8 idempotency keys and marker-bounded sections | `docs/evidence/sprint-25-multi-agent-local.md`, fake GitHub server fixtures | Fake server only; authenticated evidence is External 3 / Gate 15.1 (open) |
| CAP-15: retain failures and stop repeated nonprogress | Sprint 11 three-round limit with escalation and dispute; Sprint 26 repeated-timeout distinction; Sub-task 24.3.2.4 follow-up ceiling | `sprint-11.md`, `sprint-26-correction-timeout-hardening.md`, workflow tests `serial_campaign_reports_repeated_correction_timeout_distinctly` | Local |
| CAP-15: CI repair bound to the exact commit | Sub-tasks 24.2.2.1 to 24.2.2.5 (required checks read only for the exact PR and SHA, untrusted logs, attributable routing, truthful pause) | `sprint-25-multi-agent-local.md` | Fake CI; live CI is External 3 (open) |
| CAP-38: review the immutable candidate and evidence in a separate context | Sprint 6 read-only fresh Codex sessions; Sub-task 24.1.2.1 cumulative read-only integration review; Sub-task 24.3.2.3 final cumulative review | `sprint-6-local.md`, `sprint-6-review-scope.md`, `sprint-25-multi-agent-local.md` | Local; live reviewer is Gate 20.2 (open) |
| CAP-38: verify findings and correction limits | Sprint 11 legal finding transitions with exact correction commits and validated no-change explanations | `sprint-11.md`, review crate unit tests | Local |
| CAP-38: an implementer's model assessment is not independent acceptance | Sub-task 24.1.2.2 (no pod approves its own integration), Sprint 11 (author is never the sole final reviewer), Sub-task 24.1.2.3 stronger profiles for sensitive boundaries | `sprint-25-multi-agent-local.md` | Local; authorship-lineage independence and reviewer-turned-implementer rules are Sub-tasks 33.2.1.2 and 33.2.1.3 (open, already in the ledger) |

Outcome: every gap maps to an existing open row (Gate 15.1, Gate 20.2, External 3, Sub-tasks
33.2.1.2 and 33.2.1.3); no new row is needed and no acceptance is claimed here.

## CM-R03.4 - Engineering recipes

Implemented in `crates/codingmage-runtime/src/recipe.rs` and the `recipe-instantiate` command:

- `RecipeSpec` (version 1, `deny_unknown_fields`) declares a closed kind, bounded parameters,
  repository-relative scope roots, prerequisite gate identities, an optional verification gate and
  a rollback boundary. A recipe that is not Git-recoverable or that declares any external effect
  is refused, so the coordinator never claims rollback it cannot perform.
- Instantiation takes an operator run-spec template for providers, authentication and completion
  policy; the recipe supplies only owned paths, the reproduce-before-repair gate and a bounded
  rendered context that reaches the implementer packet as data. Every referenced gate must exist
  among the configured gates, and an existing output file is never overwritten.
- `RunSpec.context` (bounded, optional) carries the rendered recipe; ordinary units are unchanged.

Tests: recipe unit tests (instantiation under template authority, TOML round trip, eleven
refusal mutations, unknown-field refusal, context budget) and the real-process test
`a_recipe_instantiates_into_a_repair_unit_under_template_authority` (rendered spec, packet
contains the recipe as data, repair receipt, unconfigured gate refused without writing).

## CM-R03.5 - Crosswalk of controlled Git delivery grants against CAP-14

| Acceptance statement | Implementation | Evidence | Status |
| --- | --- | --- | --- |
| Branch, commit, push, PR, check and merge actions have separate exact grants | `CapabilityPolicy` grants (`network`, `push`, `issues`, `pull_requests`, `task_merge`, `destination_merge`, all denied by default), `PublicationPolicy`, campaign `publication`, `MultiAgentPolicy.publication_mode`, `task_integration_policy`, `destination_promotion_policy`, exact `GitHubCampaignPolicy` identities; preflight refuses disagreement between them | `docs/operations/configuration.md`, `docs/operations/github.md`, Sub-tasks 24.2.1.x, 24.3.1.x, `sprint-25-multi-agent-local.md` | Local; authenticated evidence is External 3 / Gate 15.1 (open) |
| Coordinator-only commits, no model Git authority | Sprint 3 worktree isolation, coordinator author identity, no provider access to the authenticated CLI | `sprint-3.md`, `docs/operations/github.md` | Local |
| Push only exact verified branches; draft PRs; no merge, release, deletion, settings, secrets or Actions administration | Sub-tasks 24.2.1.1, 24.2.1.3, 24.2.1.5 and the fixed `gh` argument vectors | `sprint-25-multi-agent-local.md`, fake GitHub server tests | Fake server only (External 3 open) |
| Separate destination promotion authority bound to reviewed commits and one create-once request | Sub-tasks 24.3.1.4, 24.3.1.5, `campaign-approve-destination` | `sprint-25-multi-agent-local.md` | Local |
| GitHub Enterprise host and CA policy | `GitHubCampaignPolicy.host` is an exact identity; no enterprise CA bundle, proxy or per-host trust policy field exists and no fixture exercises a non-`github.com` host | none | Gap: needs a new implementation row and fixtures before any claim |
| Protected-branch behavior | Protected branches are refused as publication targets (`CampaignSpec.protected_branches`, preflight `default_branch_protected`) and destination promotion requires branch protection (Sub-task 24.3.1.5) | `sprint-25-multi-agent-local.md` | Local; remote protection observation needs authenticated fixtures (External 3) |

Outcome: one genuine gap (enterprise host and CA policy) is recorded as new row CM-R03.5a in
`TASKS.md`; everything else maps to existing rows or the open authenticated-GitHub evidence.

## Open

- CM-R03.2 to CM-R03.6 remain open as listed in `TASKS.md`; CM-R03.6 needs separately admitted
  disposable credentials and a target (External 3).
