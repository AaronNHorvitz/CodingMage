# Sprint 36.3 native UI baseline and frozen qualification protocol

Prepared against source commit `57f79133cc1dc67c7b3f9953fa9fb24dbf441b22` before any
36.3 performance or human measurement. The implementation column was reconciled against
`ee97ecd5e9dc60bd75858087801d3eb58af81608` and updated with the 2026-09-28 campaign-command
preview, status-payload binding and always-visible status-bar increments, plus the subsequent
offline Help and report-freshness increments; the frozen profiles, budgets and trial protocol below
are unchanged. This is a source inventory and test plan, not a UI qualification result.
The binding requirements are
[the native UI specification](../architecture/native-ui-specification.md). Later source changes
must update the implementation column without silently changing the frozen protocol below.

## Screen and state inventory

The `Screen` enum in `crates/codingmage-ui/src/app/mod.rs` has eight destinations in the current
source. Embedded sections are listed separately because the specification requires
their own complete workflows.

| Specified screen | Current source surface | Gap against the specification |
| --- | --- | --- |
| First run | Empty Overview with repository opening, Setup and Help links, storage/sign-in guidance; `CoordinatorBinary::sibling` | No dedicated first-run flow, objective capture contract or complete per-error recovery path. |
| Workspace | Overview, diagnosis grid and selected-campaign summary | Repository and source-plan counts, selected campaign, coordinator state, recorded outcome counters, attention and freshness are present; objective capture and a chronological recent-outcome history are absent. |
| Work plan | `work_plan`, `PlanIndex`, filters, source checkbox labels and `campaign-head-plan` overlay | Search, kind/state filters, dependencies and coordinator-bound head states exist; large-list virtualization and contextual outcome history are absent. |
| Task detail | `item_detail` below the selected row, `campaign-task-detail` and status-bound `campaign-run-records` | A bounded source excerpt, source-stated criteria and selected-task run phases exist; packet and prompt text, full review findings and test logs remain unavailable from the command boundary. |
| Team activity | Campaign status, active tasks, `campaign-run-records` projection | Actor, model, pod, phase and bounded journal summaries exist; one chronological filterable activity log and complete output/timing detail are absent. |
| Blockers | Campaign `holds_section`, blocker explanation and reports | Codes and affected task exist; recovery actions and a dedicated searchable view are absent. |
| Changes and evidence | `campaign-changes` and `campaign-run-records` coordinator projections | Commit, changed files and known gate/review dispositions exist; exact diff hunks, full retained findings and logs are unavailable from the command boundary. Missing evidence is labelled. |
| Reports | Reports screen and `report::export` | Inspect/export, overwrite/privacy refusal and per-source observation freshness in view/export exist; explanatory outcome depth and broader report navigation remain. |
| Configuration | Setup and campaign readiness sections | Guided configuration, validated writes, preflight and error explanations exist; schema/default view, every policy, and full mode setup are incomplete. |
| Campaign controls | Campaign execution and durable status sections | Admission, start, stop/resume/cancel, reconnect and explicit read-only status refresh exist; diagnosis, preflight, start, four campaign controls and status refresh have exact command previews. Admission and other local actions still lack command equivalents. |
| Settings | Settings destination and design tokens | System, light, dark and high-contrast appearances work for this window; appearance controls use the bundled English catalogue. Language, storage and defaults are absent. |
| Help and About | Help destination with Ctrl+8, offline guidance, glossary, shortcuts, bundled source licence/notices, a path-free copyable diagnostic summary and a manual coordinator `support-bundle` request; informational and manual-diagnostics control labels use the bundled English catalogue | Full UI text externalization, locale formatting, complete right-to-left behavior, exact packaged third-party licence list and broader in-context diagnostics remain. Synthetic Help and Settings expansion/right alignment do not qualify these gaps. |

| Required state | Current handling | Remaining gap |
| --- | --- | --- |
| Empty / not started | Overview, work plan, campaign and report empty text | Per-screen next step and help are inconsistent. |
| Loading | `Observed<T>` and selected panels | Some partial loading states and polite accessibility announcements are absent. |
| Failure / malformed output | `BackendError`, `failure_box`, strict model parsers; Campaign status failure labels uncertain progress and offers a bound read-only refresh | Other failures remain generic and their effect on the campaign is unclear. |
| Stale / disconnected | `Observed<T>::freshness`, response-generation and binding checks; campaign status and mission payload identities are checked before acceptance; the status bar labels retained stale campaign and mission values | Not every panel labels a retained stale value; repository diagnosis and connection states still need complete recovery. |
| Executable missing | `Connection::Unavailable` in Overview/status bar; error guidance requires the sibling `codingmage` executable | First-run recovery path is incomplete. |
| Provider unavailable / authentication required / permission denied | Preflight and backend error explanations | No consistent per-screen state and direct recovery action. |
| Campaign and task outcomes | `CampaignStatus`, task overlay, selected-task run phases and independent counters | Some coordinator states are raw codes; packet, prompt, finding and log history is absent from the projection. Source checkboxes and verified outcomes are kept distinct. |

Shell gaps apply to every screen: the navigation has no command palette; the status bar now shows
bound campaign identity/state, involvement, identified active pods and the coordinator's last
checkpoint time, but the current gate remains unreported by the coordinator contract. The
minimum window is now 1024 by 640 logical pixels. Shared design tokens, themes, contrast and
offscreen screenshot evidence exist, as does the inert-content/confirmed-link adversarial matrix;
these do not qualify real desktop accessibility. Most user-facing text remains embedded in Rust;
application-wide pseudo-locale coverage, complete right-to-left behavior and locale formatting
are absent. The Help and Settings synthetic expansion and right-alignment tests above do not qualify
those gaps. Reduced motion is the default because the shell
has no essential animation. The UI still performs local configuration, browsing and process
observation; Decision 0016's command boundary and section 2 require those operations to be
reconciled, with missing commands added to `codingmage`.

## Frozen device profiles

These are **target device configurations**, not claims that those devices were tested or are
available. A measured machine must match its row's CPU class, memory, renderer, display and OS,
or receive a new profile ID before testing. P1 records the currently available isolated builder:
an Intel Core i9-13900KF Linux x86-64 host with 62 GiB physical RAM, a three-CPU quota and a
6 GiB hard memory limit. No GPU or interactive display is exposed to this lane; P1 software
offscreen results must be labelled as such and cannot qualify desktop input or Orca.

| Profile | Pinned target hardware and environment | Availability now |
| --- | --- | --- |
| P1 | Intel Core i9-13900KF, at least 32 GiB physical RAM, Linux x86-64, 1920×1080 at 100%, software renderer; app measured outside the build scope for desktop qualification | CPU/offscreen only; desktop unavailable |
| P2 | Intel Core i5-1235U, 16 GiB RAM, integrated Intel graphics, Linux x86-64, 1920×1080 at 125% and 150% | Unavailable |
| P3 | Intel N100, 8 GiB RAM, Linux x86-64, 1920×1080 at 100%, software renderer with no usable GPU | Unavailable |
| P4 | P2 CPU/RAM/graphics with a 3840×2160 display at 200% | Unavailable |
| P5 | Intel Core i5-1235U, 16 GiB RAM, integrated Intel graphics, Windows 11 x64, 1920×1080 at 100% | Platform lane unavailable |

The exact OS build, kernel, renderer version and device identifier are recorded with each future
result. A different device or driver is a separate run, not a substitution for a pinned profile.
The Windows profile remains an open future lane and does not extend the Linux demo claim.

## Frozen numerical budgets and measurement

The following numbers are fixed before measuring. A profile fails a budget when its qualifying
run exceeds it; a missing device or inaccessible measurement is **unqualified**, never pass.

| Measure | P1 | P2 | P3 | P4 | P5 | Method |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Process start to first interactive frame | 2 s | 3 s | 4 s | 3 s | 4 s | 20 cold launches; report worst and median; all must meet limit |
| Input to visible response | 100 ms | 100 ms | 100 ms | 100 ms | 100 ms | p95 of at least 100 scripted actions across navigation, search and controls |
| Live scroll frame time | 16.7 ms | 25 ms | 33 ms | 33 ms | 33 ms | p95 of at least 300 frames with live activity |
| Idle cost | 1% of one CPU core | same | same | same | same | 60 s with an open idle window, app process tree only |
| Resident memory at idle | 300 MB | same | same | same | same | peak RSS of app process after warm-up; exclude coordinator/providers |

Run the input and frame checks at 1024×640 logical pixels and each profile's listed scale.
Repeat with 10,000 tasks, 100,000 activity events and 1,000 blockers from deterministic,
synthetic data, reporting load time, p95 input/frame times, memory and truncation. The scale
case obeys the same latency and frame budgets. Use a release build, record its exact commit,
binary digest, renderer, OS and hardware, warm-up procedure, run count, raw observations and
failures. Offscreen software measurements are engineering diagnostics only when desktop input
or presentation is part of the metric. CPU, memory, coordinator and provider measurements are
reported separately; no result is silently merged across profiles.

## Frozen human trial protocol

Recruit at least eight representative first-time repository owners per round and a separate
round of at least eight experienced developers. Use the installed native app, the real backend
and disposable repositories. First-time participants receive only the objective and in-app help;
experienced participants may use the in-context depth and in-app help, but no shell instructions.
No fake-provider run is counted as a human trial or live-model qualification.

Each person attempts: open a repository, identify a dependency-ready task and its source versus
verified state, configure and diagnose a campaign, inspect exact command/authority before
admission, start and detach/reconnect, resolve a presented blocker, inspect changes/tests/review,
and export a redacted report. Record each task's completion without facilitator aid, time,
errors, navigation path, accessibility needs and observed severity. A core workflow passes a
first-time round only when at least seven of eight complete it unaided and no severity-1 finding
remains open. The experienced round must record whether all relevant detail can be found in
context and every action's command can be shown; any missing required detail is a failure to
fix and retest. Severity 1 means loss of authority boundary, data, task control or access to a
core workflow; severity 2 blocks a task with a workaround; severity 3 is confusing but usable.
Record attempted and failed runs, exact build/profile, consent and privacy-safe aggregate data.
After a fix, repeat the affected workflow with a new round; do not edit the thresholds after
seeing results. Screen-reader and clean desktop installation checks remain separate required
human evidence.
