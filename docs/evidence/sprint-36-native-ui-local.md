# Sprint 36 Native UI Local Evidence

- **Status:** Local implementation and deterministic tests only; no live-provider, installation,
  screen-reader or independent-review claim
- **Crate:** `crates/codingmage-ui` (binary `codingmage-ui`)
- **Renderer in the sandbox:** offscreen `wgpu` on the Mesa lavapipe software adapter through
  `egui_kittest`; no display server or GPU exists in the development sandbox
- **Coordinator under test:** the workspace `codingmage` binary built from the same commit

Every section names the sub-task it supports, the exact commands, and what they do not prove.

## Sub-task 36.1.1.2 - Native shell and bounded backend connection

Implemented in `crates/codingmage-ui/src/app.rs`, `backend/cli.rs`, `backend/worker.rs`,
`backend/models.rs`, `observed.rs`, `project.rs`, `state_dir.rs` and `fonts.rs`.

Behavior:

- The shell renders navigation, top bar, status bar and content panels through eframe with
  reactive repaint; a repaint is scheduled only while a request is in flight.
- The coordinator executable is resolved as the sibling `codingmage` file. A missing, linked or
  relative executable is a visible failure state and every request is refused with
  `codingmage.ui.binary_unavailable`.
- Opening a configuration validates it with `codingmage-core`, parses the task source with
  `codingmage-plan`, remembers the path in the private recent list and issues one `doctor`
  request. It starts no coordinator process and changes nothing in the target or state roots.
- Requests carry the configuration path, repository identity and campaign identity plus a
  generation counter. Responses with a stale generation or a different binding are discarded and
  counted. Cancelling advances the generation and kills the in-flight subprocess.
- Backend output is parsed with strict models; unknown fields or an unsupported schema version
  become `codingmage.ui.contract` failures shown on screen.
- Observations carry freshness (`not requested`, `loading`, `live`, `stale`, `failed`) and age.

Commands (2026-09-21, commit recorded in the handoff):

```text
cargo build -p codingmage-cli --locked
cargo test -p codingmage-ui --locked -- --test-threads=1
cargo clippy -p codingmage-ui --all-targets --locked -- -D warnings
```

Results: 12 unit tests and 8 shell integration tests passed. The integration tests build the
application inside `egui_kittest`, drive it through the AccessKit tree and the real coordinator
binary on disposable repositories:

| Test | What it proves |
| --- | --- |
| `shell_renders_navigation_and_keyboard_switches_screens_without_a_project` | Navigation buttons exist in the accessibility tree; click and Ctrl+digit switch screens; nothing is requested before a repository is opened |
| `missing_coordinator_is_a_visible_failure_state_and_refuses_requests` | Missing executable renders the failure state with the expected path; opening a project yields a `failed` diagnosis with the same code |
| `opening_a_repository_observes_real_diagnosis_without_side_effects` | Real `doctor` output is shown (repository id, head, branch, clean, denied capabilities); target and state trees are byte-identical before and after; no worktree is created |
| `invalid_configuration_is_reported_and_leaves_no_project_open` | The existing loader's stable reason is shown; no project is opened |
| `stale_generation_and_cross_project_responses_are_discarded` | A response for an earlier generation or another configuration is discarded and counted; the current binding is accepted |
| `malformed_backend_output_is_an_explicit_contract_failure` | Unknown fields and an unsupported schema version from a fake coordinator become contract failures |
| `compact_window_still_exposes_navigation_and_content` | At the 720x480 minimum size every navigation button and the open control remain in the tree |
| `software_renderer_produces_a_non_blank_frame` | The production wgpu renderer produces a real frame on the software adapter |

Not proven here: a real desktop window, Wayland or X11 input, a screen reader, installation and
any campaign behavior. Those belong to later sub-tasks and to the human-only items.

## Sub-task 36.1.2.1 - Repository selection and read-only work plan

Implemented in `crates/codingmage-ui/src/workplan.rs`, `browser.rs` and the work-plan screen in
`app.rs`.

Behavior:

- The work plan is built from `codingmage-plan::TaskPlan::parse` over the exact task-source
  bytes. Rows keep the source checkbox, kind, title, parent, sprint and story, source line and
  line digest, declared dependencies with each dependency's resolved source state, dependents,
  and a readiness derived from the source alone (`checked in source`, `dependency-ready`,
  `waiting on dependencies`).
- Search matches identifier or title; filters cover checkbox state, kind and dependency-ready
  only. Selecting a row shows its detail and can copy the identifier.
- Repository selection offers a path field, the private recent list and a directory browser that
  lists only directories and `.toml` files, skips hidden entries, disables symbolic links and
  never writes.
- A task source that fails the strict grammar is a visible failure on the overview and the work
  plan; the repository stays open for diagnosis.

Commands:

```text
cargo test -p codingmage-ui --locked -- --test-threads=1
cargo clippy -p codingmage-ui --all-targets --locked -- -D warnings
```

Results: 15 unit tests, 8 shell tests and 4 work-plan tests passed.

| Test | What it proves |
| --- | --- |
| `work_plan_shows_source_states_dependencies_and_anchors_without_editing` | Checked, dependency-ready and waiting rows, dependency states, source line and dependents appear in the accessibility tree; the target tree, head and task source are byte-identical afterwards |
| `filters_and_search_narrow_the_plan` | Ready-only, checked, acceptance and query filters narrow the rows and the counts agree with the parser |
| `malformed_task_source_is_a_visible_failure_and_overview_still_works` | A source rejected by the strict grammar is reported on both screens without closing the repository |
| `browser_lists_directories_and_opens_a_configuration` | The browser exposes navigation controls and lists only directories and configuration files |

Not proven here: coordinator observations over the plan (Sub-task 36.1.2.2) and any file dialog
integration with the desktop environment.

## Sub-task 36.1.2.2 - Campaign and team state with distinct outcome states

Implemented in `crates/codingmage-ui/src/campaign.rs`, `app/campaign_screen.rs`, the read-only
Git job in `backend/cli.rs` and `backend/worker.rs`, and `state_dir.rs` (per-configuration
campaign memory).

Behavior:

- A campaign specification is loaded with `CampaignSpec::load`, its authority digest computed,
  and it is refused before any backend request when its repository path or repository identity
  differs from the opened repository.
- `campaign-status`, `campaign-explain-blocker` and (for parallel campaigns) `campaign-report`
  are requested through the coordinator binary and polled every 15 seconds with reactive
  repaint. `null` status is an explicit "never started" state, not an empty table.
- The task source at the campaign head is read with `git show <head>:<task_source>` (cleared
  environment, no pager, no hooks) and parsed with the same strict parser, so verified completion
  at the campaign head is shown separately from the active checkout's source checkbox.
- The per-task overlay keeps source checkbox, campaign-head completion, accepted outcome (report),
  active unit, blocked reason, deferral trigger state and human-decision reason as separate labels;
  a task without any coordinator observation is labelled "coordinator state unknown".
- Binding drift between the active checkout (repository identity, head, task-source digest) and
  the campaign authority is reported, never corrected.
- Only the roles the backend reports (`coordinator`, `codex-lead`, `pod`, `integration`) are
  listed; supervised, exception-only and hands-off modes are shown as unavailable with the reason.
- Repository-level observations remain valid when a campaign is selected afterwards; campaign
  observations are bound to the selected campaign identity.

Contract correction discovered by the strict models: `campaign-status` emits schema version 5 and
`campaign-preflight` emits schema version 2; the reconciliation table was corrected and the
accepted versions are pinned in `backend/models.rs`. Contract-parity tests serialize the real
runtime types into the interface models.

Commands:

```text
cargo test -p codingmage-ui --locked -- --test-threads=1
cargo clippy -p codingmage-ui --all-targets --locked -- -D warnings
```

| Test | What it proves |
| --- | --- |
| `never_started_campaign_is_an_explicit_empty_state` | A selected campaign without durable state shows the explicit never-started state, the unavailable modes and the binding match; no campaign directory is created |
| `completed_unit_is_distinct_from_the_source_checkbox_and_counts_agree` | After one real accepted unit through the coordinator with fake providers, the status counts (1 of 1 accepted, paused) agree with the CLI outcome; the work plan shows "open in source" plus "completed at campaign head (verified, not yet in active checkout)" for the accepted task only |
| `blocked_task_shows_its_closed_reason_and_independent_progress` | A typed lead blocker appears with its closed reason while the independent task completed |
| `cross_repository_campaign_is_refused_before_any_backend_request` | A specification for another repository path, and one tampered to the right path but wrong identity, are refused with distinct reasons and no status request |
| `campaign_selection_is_remembered_per_configuration` | The selection is restored on reopen from private state and cleared explicitly |
| `contract_parity` (4 tests) | Real `CampaignStatus`, `CampaignBlockerExplanation`, `CampaignOutcome`, `CampaignControlOutcome`, `CampaignPreflightReport` and `TeamCampaignReport` values round-trip through the interface models |

Not proven here: starting or controlling a campaign from the interface (Story 36.2) and any
parallel-campaign report rendering over a real parallel run.

## Sub-task 36.1.3.1 - Guided configuration and campaign authoring

Implemented in `crates/codingmage-ui/src/setup.rs` and `app/setup_screen.rs`.

Behavior:

- The configuration form starts from the same deny-first defaults as `codingmage init` (two
  profiles, one `git diff --check` gate, local-only publication, every capability denied) or from
  the opened configuration. Writing serializes the existing `Config` type, writes a private
  candidate file, runs the existing `load_config` on the exact bytes and only then renames it into
  place; a loader rejection (for example a publication policy that conflicts with denied
  capabilities) leaves the previous file untouched. Missing scratch and state roots are created.
- The owner's authorization record is typed by the owner, written with its exact bytes outside
  the repository, and its digest is bound into the campaign authority. Records inside the target
  repository are refused, matching the preflight rule.
- The campaign form binds repository identity, head and task-source digest from the live
  diagnosis rather than typed text, offers only serial, one-pod, local-only authority with the
  closed effort list, verifies the built `CampaignSpec`, writes a candidate, reloads it with
  `CampaignSpec::load` and then selects it. Parallel pods and draft pull requests are named as
  import-only.
- Model selectors are free text passed to the provider unchanged; the interface keeps no model
  list and states that preflight probes validate them.
- Export copies a validated file to a new path with the same overwrite and inside-repository
  refusals. No field asks for or stores a credential.

| Test | What it proves |
| --- | --- |
| `guided_configuration_is_validated_by_the_existing_loader_and_opened` | The guided file loads with the existing loader, opens as the project, leaves the target untouched; a conflicting policy is refused with the loader's reason and the file is unchanged; overwrite is refused without consent |
| `guided_campaign_binds_the_live_diagnosis_and_refuses_records_inside_the_repository` | Authorization records inside the repository are refused, outside they are written byte-exact; the written specification carries the diagnosis identity, head and digest, verifies through the existing loader and becomes the selected campaign; export refuses the repository and silent overwrite and copies bytes exactly |
| `campaign_form_without_a_live_diagnosis_is_refused` | Without a live diagnosis the campaign form cannot be applied |
| `setup` unit tests (2) | Writers refuse overwrite, keep the previous file on rejection and bind the authorization digest |

Not proven here: preflight over the authored files (Sub-task 36.1.3.2).

## Sub-task 36.1.3.2 - Readiness and preflight

Implemented in `crates/codingmage-ui/src/readiness.rs` and `app/readiness_screen.rs`.

Behavior:

- Local checks mirror the conditions the coordinator's preflight enforces and explain its stable
  codes before it runs: repository identity, initial commit, task-source digest, clean checkout,
  dedicated branch, protected default branch, at least ten open sub-tasks, controlled-target
  authority (one pod, exactly ten accepted outcomes, local-only publication, existing-login
  authentication, no multi-agent policy), external capabilities denied, provider executables
  present, and the authorization record matching the bound digest outside the repository. Each
  check carries the observed detail and an action; unknown conditions are labelled unknown.
- `campaign-preflight` runs through the coordinator with the selected authorization record. The
  parsed report (schema 2) is shown with the SHA-256 of the exact bytes the coordinator wrote,
  which later admission binds to. Failures show the stable code with an explanation; provider
  probe failures name the configured provider and state that nothing is substituted.
- Preflight never starts inference and leaves no campaign state; the tests assert the campaign
  state directory does not exist afterwards.
- The authorization record path is remembered per configuration in private state.

| Test | What it proves |
| --- | --- |
| `local_checks_explain_the_preflight_failure_before_it_runs` | A default-branch, three-sub-task, two-outcome campaign fails the dedicated-branch, open-sub-task and controlled-target checks with exact details; a mismatched record is a distinct failure; running preflight anyway returns `codingmage.runtime.authority` with its explanation and creates no campaign state |
| `real_preflight_passes_on_a_ready_fixture_and_binds_the_report_digest` | On a ten-sub-task fixture on a dedicated branch with fake providers, every local check passes and the real preflight reports `ready`, verified providers, a dedicated branch and a source-free report whose bytes contain neither the repository path nor model names |
| `provider_probe_failure_is_actionable_and_names_no_substitute` | An implementer executable without the required capability surface fails preflight with a `codingmage.provider.claude.*` code and the interface states that no substitute is used |

Not proven here: authenticated real providers; the probes ran against fake executables.

## Sub-tasks 36.2.1.1 and 36.2.1.2 - Admission, controls, detach, reconnect and recovery

Implemented in `crates/codingmage-ui/src/admission.rs`, `launch.rs`, `controls.rs` and
`app/execution_screen.rs`.

Behavior:

- Admission requires a `ready` preflight report whose authority and authorization digests match
  the selected specification, and the owner must type at least the first twelve characters of
  the report's SHA-256 exactly as shown. The admission is persisted privately, bound to the
  authority digest, repository identity, head, task-source digest and authorization digest, and
  becomes stale (start refused) when any of them change. It grants nothing: the coordinator
  revalidates everything when `campaign` starts.
- Start launches `codingmage campaign` in its own process group with stdin closed and private
  stdout/stderr files, records pid and kernel start time, and never waits for or signals it.
  Liveness is read from `/proc` and the recorded start time; the terminal JSON outcome is read
  from the private stdout file after exit. A second start is refused while the launch is live,
  and the coordinator's own repository lock remains the authority.
- Pause, resume, stop-after-unit and cancel go through `campaign-control` with interface-generated
  create-once request identities kept in a private ledger. A pending request blocks another;
  a lost outcome is replayed with the same identity and the coordinator answers
  `created: false`; cancel needs a second press; resume records the intent only and starting
  again is a separate explicit action.
- Closing the window neither stops nor adopts the coordinator. Reopening restores the admission,
  launch record and ledger, observes the live process, and marks any pending request whose
  outcome was lost as replayable.

Commands:

```text
cargo test -p codingmage-ui --test execution --locked -- --test-threads=1
```

| Test | What it proves |
| --- | --- |
| `admission_requires_the_reviewed_report_and_goes_stale_when_the_repository_moves` | Admission is refused without a report or with a wrong digest prefix; a confirmed digest admits; the admission survives reopening; a new commit on the checkout makes it stale and start is refused without creating campaign state |
| `start_detach_reconnect_stop_after_unit_and_replayed_controls` | The real coordinator starts detached with fake providers; closing the interface leaves it alive; a new interface reconnects to the same pid; stop-after-unit is created, the coordinator exits `paused`/`stop_after_unit` with at least one accepted unit; a pause whose outcome is marked lost is replayed with the same identity and reported as already recorded; resume records the intent and start becomes available again; the active checkout's task source is untouched |
| `cancel_terminates_only_the_owned_provider_and_retains_state` | With a provider that sleeps, cancel (after confirmation) makes the coordinator exit `cancelled`, the owned provider process is gone, the interface process is unaffected, durable campaign state is retained and start is refused for the cancelled campaign |
| `a_second_launch_is_refused_while_the_coordinator_is_live` | A second start while live is refused locally with the pid unchanged; a control issued before the first checkpoint exists is reported with the coordinator's `codingmage.runtime.state` code; the campaign is cancelled to clean up |

Not proven here: behavior on a real desktop session (window close through the compositor) and
any live-provider execution; the coordinator ran with fake providers.

## Sub-task 36.2.2.1 - Exact changes, review and test records, bounded activity

Implemented in `crates/codingmage-ui/src/records.rs` and `app/changes_screen.rs`, with a
records scan job in `backend/worker.rs`.

Behavior:

- Exact candidate changes are read from the repository's own objects with read-only Git:
  `log` of the coordinator commits between the campaign's initial commit and its head, and
  `diff --numstat -z` of the changed files. The screen states that the campaign head equalling
  the initial commit means no reviewed candidate was integrated.
- Review and test results come from every `runs/<run_id>/checkpoint.json` beneath the campaign's
  private state, parsed with the strict checkpoint model, plus each run's journal parsed with the
  existing `JournalRecord` type. A missing checkpoint, a missing verdict, empty gate evidence, a
  missing journal or malformed journal lines are reported as exactly that, never as a pass.
  Reviewer finding text is stated as not retained by the backend.
- The delivery boundary is stated on the screen: accepted work lives on the coordinator-owned
  campaign branch under the campaign's publication policy; engineering completion is not delivery.
- Bounded activity is the coordinator's own content-minimized progress stream from the private
  stderr capture of the launch, limited to the last twelve lines.

| Test | What it proves |
| --- | --- |
| `accepted_units_show_exact_commits_files_verdicts_and_gate_evidence` | After two real accepted units, the four coordinator commits, the two changed files, per-run `pass` verdicts, two gate evidence identities each and the journaled phase sequence appear; the active checkout head is unchanged |
| `missing_and_malformed_records_are_reported_not_passed` | A run with its checkpoint removed and a malformed journal line shows "checkpoint.json is absent", "review outcome not recorded" and the malformed-line count, with the task identity recovered from the journal |
| `never_started_campaign_has_no_changes_or_records` | Never-started campaigns show explicit empty states for changes, records and activity |

Not proven here: parallel-campaign per-pod record layouts (the scan is recursive and bounded,
but only serial runs were exercised) and any real reviewer output.
