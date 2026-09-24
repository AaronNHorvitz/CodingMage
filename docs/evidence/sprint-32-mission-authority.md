# Sprint 32 Mission Authority and Owner Involvement - Local Evidence

- **Status:** Local implementation with deterministic and fake-provider tests only. No live
  provider, independent review, human acceptance or no-intervention qualification is claimed.
- **Work package:** CM-R02 (CAP-09, 33, 34, 35, 36, 42); Decision
  [0018](../decisions/0018-mission-charter-and-involvement-contracts.md)
- **Source revision:** the commit that carries this document; test receipts below name it

## Sub-task 32.1.1.1 - Inventory and owner mapping

| Charter element (team contract) | Existing owner reused | Gap closed in this sprint |
| --- | --- | --- |
| Campaign identity, commit, task-source and operator-authorization digests | `CampaignSpec` and `authority_sha256` in `codingmage-campaign` | Charter binds all of them plus the campaign authority digest (`MissionCharter::verify`) |
| Allowed roots, denied roots, gate tiers, provider profiles, limits | `CampaignSpec` | Charter domains and command registry are validated as subsets; nothing widens |
| Durable, integrity-protected, replay-safe controls | `team-control.json`, `IntegrityDocument`, `CoordinatorLock` | `missions/<campaign_id>/mission.json` follows the same pattern |
| Typed lead dispositions | `TeamLeadReport`, `HumanDecisionBlocker`, serial `human_decisions` projections | Optional typed `decision` on the human-decision payload; mission evaluation before the legacy path |
| Effect revalidation before each unit | `observe_team_control`, `campaign_control_termination` | Mission observation at the same boundaries in both engines and in the cancellation watcher |
| Status and report surfaces | `campaign-status`, preflight report | `campaign-mission-status`, preflight `mission` section |
| Evidence freshness prerequisite | Task 25.2.4.6 | Preserved; CM-R01 records the exact blocker (external review record) |

External blockers recorded, not treated as done: live provider runs, real desktop, independent
review, package renewal (CM-R01.6). Stale historical reports are not current qualification.

## Sub-tasks 32.1.1.2 and 32.1.1.3 - Charter schema and derived authority

Implemented in `crates/codingmage-campaign/src/mission.rs` and
`crates/codingmage-contracts/src/campaign.rs`:

- `MissionCharter` (version 1) with `deny_unknown_fields`; closed `InvolvementMode`,
  `DecisionClass`, `EscalationDisposition`; `DecisionDomainGrant` with approved alternatives,
  campaign-permitted scope paths, maximum risk, required gate tiers and escalation;
  `MissionBudgets`; issue/expiry times and revocation epoch.
- `MissionCharter::verify` refuses a charter for another campaign, commit, task source,
  authorization or authority digest, an unknown gate profile, a scope outside campaign
  authority, an `ask_owner` escalation under hands-off, and conflicting or unbounded values.
- `evaluate_decision` derives the only permitted outcomes from a verified charter, an untrusted
  `DecisionProposal` and coordinator observations: `permitted_choice`, `decision_needed`
  (never under hands-off), `deferred` or `blocked`. Class, path, risk or gate expansion is
  blocked in every mode; unapproved alternatives follow the domain escalation without widening
  it; expired, revoked, budget-exhausted and no-progress states never permit a choice.

Tests (`cargo test -p codingmage-campaign mission`): round trip and digest stability, unknown
field rejection, sixteen binding/limit mutations, hands-off prompt refusal, permitted choice in
every mode, undelegated choice per mode, eight expansion mutations in every mode, escalation
matrix, expiry/revocation/budget/no-progress holds, stale epoch and malformed proposal errors,
file loading rules.

## Sub-task 32.1.1.4 and 32.2.2.1 - Durable generations, revocation and expiry

Implemented in `crates/codingmage-runtime/src/team_mission.rs`:

- `mission.json` under `state_root/missions/<campaign_id>` is an `IntegrityDocument` bound to
  the campaign authority digest, campaign and repository identities, mission identity and the
  charter digest; it retains the admitted charter, every generation, revocation requests,
  decision records, pending owner decisions and owner answers.
- Admission is idempotent by charter digest. Re-issue requires the same mission identity, a
  higher generation, the same revocation epoch and no prior revocation; anything else changes
  nothing. Revocation is authenticated, idempotent by request identity, irreversible and
  advances the epoch once.
- Both engines call `observe_mission_authority` before admitting work; a revoked mission ends the
  invocation as `cancelled` / `mission_revoked`, an expired one as `blocked` / `mission_expired`,
  and the team cancellation watcher cancels in-flight owned processes on revocation.
- A document with an unknown version or mutated content is a hold (`codingmage.runtime.state`),
  never treated as absence.

Tests (`cargo test -p codingmage-runtime team_mission`): projection and identity mutations,
revocation idempotence and epoch, expiry observation, re-issue rules, cross-project and tampered
documents, incompatible version refusal, revoked authority across reload.

## Sub-tasks 32.1.2.1, 32.1.2.2 and 32.1.2.4 - Involvement contracts and admission preflight

- `InvolvementMode` is a charter field independent of pod count, serial/parallel execution and
  publication/promotion policy; without a charter no mode is claimed and legacy behavior is
  unchanged (existing campaigns keep their authority digest).
- `campaign_mission_preflight` reports role executable availability and digests, authentication
  boundary, retained holds derived from the integration/promotion/publication policy, the
  interactive prerequisites of the mode and exact no-intervention defects. A hands-off charter
  with any defect (expired, login discovery not configured, unavailable role executable,
  configured owner prompt) is refused with `codingmage.runtime.authority` before any process.
- CLI: `campaign-preflight ... [--mission]`, `campaign ... [--mission]` (preflight then admit,
  then run), `campaign-mission-admit`, `campaign-mission-status`, `campaign-mission-revoke`,
  `campaign-mission-answer`. The preflight report gains an optional `mission` section that is
  absent when no charter is supplied; the native workspace model accepts it as optional.
- Retained state and reports contain no charter prose, model names or credentials in status,
  preflight or control outputs; the admitted charter text lives only in private mission state.

Tests (`cargo test -p codingmage-cli --test campaign_mission`): admission/idempotence/status,
re-issue and stale generations, unbound/prompting/unknown-field charters, revocation lifecycle,
answer refusal without a pending decision, real coordinator process stopped by revocation
before any provider runs, expired charter hold and hands-off refusal.

## Sub-tasks 32.2.1.1 to 32.2.1.3 - Noninteractive decisions

- The lead may attach a typed `decision` to a human-decision disposition (schema and prompt
  updated in `codingmage-codex`). With a bound mission the coordinator records the outcome:
  a permitted choice is fed back to the next lead binding as an accepted decision and planning
  is retried; `decision_needed` registers one deduplicated pending owner decision (supervised
  and exception-only only); deferred and blocked outcomes retain the task through the existing
  human-decision projection with a mission hold code. Hands-off never registers a pending owner
  decision and issues no prompt or stdin wait.
- Consecutive permitted-choice re-plans without admitted work are bounded by
  `max_no_progress_cycles` (`codingmage.campaign.mission_no_progress` /
  `codingmage.team.mission_no_progress`); repeated identical proposals are bounded by
  `max_decision_retries`.
- Owner answers select an approved alternative or block; the answer is durable and the same
  decision identity resolves to that alternative afterwards.

Tests: decision ledger deduplication and generation binding, hands-off holds, no-progress
bound, revoked mission holds, lead routing and binding feedback (`team_mission` unit tests).

## Verification receipt

Executed inside the sandbox on the tree of the commit carrying this document, through the shared
build reservation with one build job and one test thread (private log `full-gates-8.log`):

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | no findings |
| `cargo test -p codingmage-campaign mission` | 12 passed |
| `cargo test -p codingmage-runtime` | 117 passed (team_mission: 12) |
| `cargo test -p codingmage-cli --test campaign_mission` | 7 passed, including two real coordinator-process runs |
| `cargo test -p codingmage-ui --lib --test campaign` | 27 and 5 passed (offscreen software renderer) |
| `cargo test --workspace --all-targets --no-fail-fast` | 463 passed, 3 sandbox-environment failures unrelated to this sprint (see `cm-r01-reconciliation.md`), 2 guarded ignores |
| `python3 -m unittest discover -s tests -p 'test_*.py'` | 40 pass, 1 designed freshness failure (CM-R01.6) |
| `python3 scripts/docs_check.py`, `python3 scripts/check_architecture.py`, `git diff --check` | pass |

Fake providers only; no live model, real desktop or independent review is claimed.

## Not claimed

- 32.2.1.3 stays open: the serial engine already continues independent work after a recorded
  human decision, but the parallel team engine still ends the invocation on any nonexecution
  disposition, and the reconsideration-on-changed-observation and retry/replanning bounds are
  not yet exercised end to end.
- 32.2.1.4 (late provider approval/login, quota, unknown tool, unavailable reviewer injection),
  32.2.2.2 (revocation races against implementation/commit/gate/review/integration) and
  32.2.2.4 (all three modes through pause/resume/stop/cancel with coordinator-process fixtures)
  remain open until their fixtures exist; the real-process tests here cover revocation and
  expiry holds before a provider runs.
- AC 32.1, AC 32.2, Gate 32.1 and Gate 32.2 need independent review and are not ticked.
