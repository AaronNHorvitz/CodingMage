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
