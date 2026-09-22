# Sprint 36 Native UI Reconciliation

- **Status:** Sub-task 36.1.1.1 reconciliation; no UI qualification claimed
- **Source revision:** `1d91c8c` (accepted planning documents) on branch `build/native-ui`
- **Executed:** 2026-09-21, inside the assigned sandbox with no display server or GPU

## Contracts the Native UI Uses

Every row names the exact existing surface, its authority class and how the UI consumes it.

| Surface | Kind | Authority | UI use |
| --- | --- | --- | --- |
| `codingmage init` | CLI, creates one deny-first configuration file | Writes only the selected new file | Guided setup |
| `codingmage doctor` | CLI JSON (`schema_version` 1) | Read-only repository authorization and inventory | Overview: repository identity, head, branch, cleanliness, counts, redacted configuration |
| `codingmage-plan::TaskPlan::parse` | Library | None | Work plan: sprints, stories, items, checkbox state, dependencies, source anchors |
| `codingmage-core::load_config` | Library | None | Validate authored or imported configuration before use |
| `codingmage-campaign::CampaignSpec::verify` | Library | None | Validate authored or imported campaign authority |
| `codingmage campaign-preflight` | CLI JSON (`CampaignPreflightReport`, schema 1) | Read-only probes of providers, gates, guard and storage | Readiness screen; admission requires an acknowledged report digest |
| `codingmage campaign` | CLI process, JSON outcome (`CampaignOutcome`) | Coordinator execution under the repository lock | Launched detached; owns execution; never waited on by the window |
| `codingmage campaign-status` | CLI JSON (`CampaignStatus`, schema 1) or `null` | Read-only durable checkpoint projection | Campaign, team, blockers, outcome counts, limits, utilization |
| `codingmage campaign-explain-blocker` | CLI JSON (`CampaignBlockerExplanation`) | Read-only projection | Blocker detail |
| `codingmage campaign-report` | CLI JSON (`TeamCampaignReport`) or `null` | Read-only final report (parallel campaigns only) | Reports screen and export |
| `codingmage campaign-control` | CLI JSON (`CampaignControlOutcome`) | Create-once durable control intent | Pause, resume, stop-after-unit, cancel |
| `/usr/bin/git` read-only object reads (`show`, `diff --numstat`, `log`) | Fixed executable with cleared environment | None (object database reads) | Exact candidate changes between initial commit and campaign head; campaign-head task source |
| `<state_root>/runs/<run_id>/checkpoint.json` | Private JSON (`schema_version` 1) written by the run port | None (same-user read) | Review verdict, correction rounds and gate evidence identities for an exact candidate commit |

Not used: `codingmage run` (supervised one-unit path; the UI presents campaigns),
`campaign-clear-blocker`, `campaign-observe-trigger`, `campaign-approve-task` and
`campaign-approve-destination`. Those remain CLI-only in this milestone because each binds
operator-controlled evidence digests or remote effects that the UI cannot verify truthfully yet;
the UI shows the exact command family that a person must run.

## Execution Ownership

- `codingmage campaign` is not a daemon. It runs bounded units, applies pending control intents
  at admission boundaries, watches for cancellation, and exits with a JSON outcome on completion,
  pause, stop-after-unit, block or cancel. A paused campaign has no live process; resuming means
  recording the `resume` intent and launching `campaign` again.
- One coordinator per repository is enforced by the kernel lock under `<state_root>/campaign-locks`.
  A second launch fails closed with `codingmage.runtime.orchestration`.
- Control intents are private files under `<state_root>/campaigns/<campaign_id>`; replaying the
  exact request is idempotent (`created: false`) and reusing a request identity for a different
  action fails closed.
- The binary's parent directory is the source root for the self-target check and the same binary
  is the process guard. The UI therefore runs the installed `codingmage` executable instead of
  linking the runtime.

## Known Source and Evidence Drift

- Sub-task 25.2.4.6 remains open: the multi-agent evidence binding fails freshness for
  `crates/codingmage-campaign/src/team.rs`, `README.md` and `SECURITY.md`. The Python suite
  therefore reports 38 tests, 37 passing, one failure before this workstream started. This
  workstream neither refreshes those hashes nor counts the failure against UI work.
- `README.md` lists `schemas/` and `fixtures/` directories in the repository layout; neither
  exists at this revision. The UI documentation does not repeat that layout.
- The planned attachable terminal dashboard (`codingmage monitor`) described in `README.md` does
  not exist; the monitor crate provides in-process stream and control types only. The native UI
  does not depend on it.

## Backend Gaps That Affect Truthful Presentation

| Gap | Disposition |
| --- | --- |
| `CampaignPreflightReport`, `CampaignOutcome`, `CampaignControlOutcome` and `TeamCampaignReport` derive `Serialize` only | The UI keeps strict mirror models with `deny_unknown_fields` and `schema_version` checks; parity tests serialize the real types into them. No backend change required. |
| Reviewer findings are not retained durably; only the review verdict, correction rounds and gate evidence identities are checkpointed per run | The UI shows the verdict and identities and labels findings as "not retained by the backend". No fabricated findings. |
| Serial `campaign-status` has no per-task completion list | Verified completion is read from the campaign head's task source (`git show <head>:<task_source>`), parsed with the same parser, and shown separately from the active checkout's source checkboxes. |
| `campaign-preflight` returns `codingmage.runtime.plan` for both digest drift and fewer than ten open sub-tasks, and `codingmage.runtime.authority` for several distinct branch and identity conditions | The UI runs the same local pre-checks it can evaluate without authority (open sub-task count, branch versus default and protected branches, digest match) and reports the exact failing condition before invoking preflight. |
| Involvement modes and team roles beyond lead, pod, reviewer and integration have no contract | Shown as unavailable with the reason; no mode selector is offered. |

No backend defect requiring a code change was found during reconciliation. Any later fix needed
for truthful presentation will be committed separately with a regression test.

## Compatibility Approach

- The UI is versioned with the workspace and targets the CLI contracts of the same revision.
  Backend JSON is parsed with strict models; unknown fields or an unexpected `schema_version` are
  reported as an explicit "unsupported backend output" failure state.
- Configuration version 1 and campaign version 3 are the only authored formats. Import of a
  configuration or campaign file uses the existing loaders and shows their stable error codes.
- Private UI state lives under `<state_root>/ui/` with `0700`/`0600` permissions and never inside
  the target repository or the coordinator's campaign directories.

## Verification Performed for This Sub-task

| Check | Result |
| --- | --- |
| Read of the required documents and the CLI, service, monitor, project, contracts, state, core, plan, campaign and runtime public surfaces | Completed; contracts above are taken from the source, not from historical reports |
| `python3 -m unittest discover -s tests -p 'test_*.py'` on the unchanged baseline | 38 tests, 37 pass, one pre-existing failure (25.2.4.6) |
| Dependency licence audit of the locked graph for `codingmage-ui` | 210 new runtime crates, 23 dev/build-only crates, all within the admitted licences; see Decision 0015 |
| `cargo build -p codingmage-ui --locked` with the scaffold | Passed at three jobs |
