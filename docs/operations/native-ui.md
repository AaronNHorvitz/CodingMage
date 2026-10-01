# Native Linux Desktop Workspace

## Status

`codingmage-ui` is the native Linux desktop workspace over the existing coordinator. It is
implemented locally with deterministic, display-less tests against the real `codingmage` binary
and fake providers (Sprint 36 in [TASKS.md](../../TASKS.md)). It has not been verified on a real
desktop session, with a screen reader, from a clean installation, or with real providers; those
items are listed in [the human-only register](../evidence/sprint-36-human-only.md). Incremental
independent review findings were corrected in later commits. The read-only independent review of
`ee97ecd5e9dc60bd75858087801d3eb58af81608` passed its bounded run-record and path-component
scope with independent Cargo checks; full UI independent acceptance remains open.

## What It Is

- A client of the `codingmage` executable installed next to it (Decision 0016). Campaign
  execution and controls run through that binary; the interface parses its JSON and stable
  error codes and never links the runtime. Read-only campaign-head task states, changes and
  run-record evidence also use this command boundary.
  Configuration writes and local browsing still occur in the UI process. Migrating those
  operations is open under the [native UI specification](../architecture/native-ui-specification.md).
- An observer and control surface. Opening the app or a repository starts no agent, edits no task
  status and authorizes no campaign. Closing the window never stops a coordinator.
- Built with `egui`/`eframe` on `wgpu` with AccessKit accessibility (Decision 0015), reactive
  repaint, Wayland and X11 backends, and system fonts.

## Requirements

- Linux with a Wayland or X11 session.
- `codingmage` and `codingmage-ui` built from the same commit and installed in the same
  directory (`cargo build --locked --release -p codingmage-cli -p codingmage-ui` produces both in
  `target/release`).
- A sans-serif TrueType or OpenType font in a standard font directory (Liberation, DejaVu, Noto,
  Open Sans, Cantarell or Ubuntu are found automatically), or `CODINGMAGE_UI_FONT` pointing at a
  font file. Without a font the app exits with status 2 and an explanation.
- Vulkan or OpenGL through Mesa or a vendor driver for the window renderer.

## Local development launch

From this repository, build both executables from the same source tree and start the UI in an
ordinary Linux desktop session:

```sh
export CARGO_BUILD_JOBS=1
cargo build --locked -p codingmage-cli -p codingmage-ui
target/debug/codingmage-ui
```

The application opens without starting a campaign. Select an existing validated configuration
in Setup, or create one there for a disposable repository. The native window needs a Wayland or
X11 session; this worker's headless offscreen tests do not qualify the commands above on an
interactive desktop. The first screen explains a missing sibling executable or font instead of
showing sample results.

## Screens

| Screen | Content | Source |
| --- | --- | --- |
| Overview | First-run storage and provider sign-in guidance; configuration summary, repository diagnosis (identity, head, branch, cleanliness, denied capabilities), task-source counts | `codingmage doctor`, `codingmage-core`, `codingmage-plan` |
| Work plan | Searchable, filterable plan with dependencies, source anchors, readiness from the source, disabled source checkboxes and the coordinator overlay (completed at campaign head, accepted, active, blocked, deferred, human decision, unknown) | task parser, `campaign-status`, `campaign-head-plan` for authorized, content-minimized head states |
| Campaign | Campaign authority, binding drift, readiness checks, preflight, admission, coordinator process state, controls, durable status, holds, utilization, roles the backend reports, unavailable involvement modes; Setup and Help navigation when no repository is open | `campaign-preflight`, `campaign-status`, `campaign-explain-blocker`, `campaign-control`, `/proc` |
| Blockers | Searchable bound campaign hold, blocked, deferred and human-decision codes; explicit empty, stale and failed observations; read-only exact-command refresh and code-specific recovery guidance | `campaign-explain-blocker`; eligible external clearance and trigger observations still require operator-supplied evidence through coordinator commands, while coordinator-observed triggers are never manually marked |
| Changes and reviews | Delivery boundary, coordinator commits and changed files, per-run verdicts and gate evidence, journaled phases, bounded activity | `campaign-changes`, `campaign-run-records` |
| Reports | Outcome and blocker reports with export and explicit source-observation freshness | the observations above |
| Setup | Open or create a configuration, write the owner's authorization record, author a campaign, import and export | `codingmage-core`, `codingmage-campaign`, `codingmage init` |
| Settings | Light, dark, high-contrast or system appearance for the current window | Local presentation state only |
| Help and About | Offline getting-started and recovery guidance, keyboard shortcuts, glossary, source licence and third-party source notices; explicit copyable path-free diagnostic summary and redacted support bundle for a selected campaign | Bundled first-party text, plus the coordinator's `support-bundle` command only after an explicit request; nothing is uploaded |

Work plan shows coordinator outcome badges only from a live, completed status
observation. While a refresh is pending, after a failure, or when status has
aged, retained task outcomes are hidden and the badges read unknown; source
checkboxes remain unchanged. **Refresh coordinator outcomes** is a read-only
`campaign-status` request with the exact command shown beside it. An unstarted
campaign and a malformed task source have separate explanations, and the empty
and failed plan views lead to Setup and offline Help.
Final-report accepted badges also require the selected report's campaign,
repository, base and branch binding and a live completed status at the same
branch and head. A pending, failed or stale report refresh or changed head hides
those badges; the source checkbox remains a separate claim. Work plan shows the
final report observation's freshness and its failure effect. **Refresh final
report** makes a read-only `campaign-report` request for a parallel campaign;
its exact command is shown beside the action. An observed missing report is
distinct from a failed request and cannot supply accepted badges.

The Reports screen can explicitly inspect the read-only
`codingmage campaign-outcome-report --config <absolute-file> --campaign
<absolute-file> --include-paths false` command. It shows a bounded, inert
source-bound snapshot and its exact command without starting an agent or
writing a destination. Select Inspect again for newer records. The Reports
export control runs `codingmage report-export`
with the shown config, campaign, output, privacy and overwrite arguments. It writes a fresh
source-bound schema-one document. Changed-file paths require `--include-paths true`;
replacement of an existing regular file requires `--overwrite true`. The document never
claims an interface-only admission or invocation result. The Reports screen also shows a
separately labelled local schema-three observation JSON; it is not the exported document.
Source inspection and export remain available while this local observation
assembles or becomes stale; each asks the coordinator to read fresh bound
records independently.
The export writers require the selected destination parent to be outside the target
repository and on the same Linux mount as its root. A destination on another mount is
refused, even when outside the repository.

## Workflow

1. Open a configuration (path, recent list or browser), or choose a repository directory in
   Setup and write a deny-first configuration; the existing loader validates it first.
2. Write the owner's authorization record outside the repository and author a campaign; the
   repository identity, head and task-source digest come from the live diagnosis.
3. On the Campaign screen, review the local readiness checks, run preflight, and admit the
   campaign by typing the first twelve characters of the report digest. Admission records your
   review; it grants nothing.
4. Start the coordinator. It runs `codingmage campaign` in its own process group; the interface
   shows its exact command on request, pid, activity lines and outcome, and you may close the
   window at any time.
5. Pause, resume, stop after the current unit or cancel through `campaign-control`. Cancel
   needs a second press. Each control can show its exact command and bound request identity
   before submission. Resume records the intent; press Start again to continue a paused campaign.
6. Inspect changes, review and test records, and export a report to a file in an existing
   directory outside the repository. A linked parent that resolves into the repository or
   changes identity during an observed export step is refused. Repository file paths are
   excluded unless you opt in. An export without overwrite consent cannot replace a file
   created concurrently at the destination. A report's changed-file count is "not observed"
   until its coordinator projection arrives; a truncated projection is labelled a lower bound.
   Export is dispatched from the bounded background worker to a supervised one-shot writer
   process. Wait for its result before relying on the file; changing the selected repository
   or campaign signals cancellation, though a write may have completed before that signal
   was observed. Changing screens alone does not cancel it.

## Keyboard

- `Ctrl+1` to `Ctrl+8` switch existing screens; `Ctrl+9` opens Blockers. `Tab` and
  `Shift+Tab` move focus; `Space` or `Enter` activate. `F5` refreshes the diagnosis and
  campaign observations.

The native minimum window is 1024 by 640 logical pixels. Appearance uses shared visual tokens;
its choice lasts until the window closes. Reduced motion is the default because the shell has no
essential animation. The six appearance baselines are offscreen software renders; see
[the Story 36.3 evidence](../evidence/sprint-36-appearance.md).
The bundled English catalogue supplies static Help, Settings, Workspace, Blockers, Reports,
Campaign and Work plan orientation and recovery labels, plus selected dynamic Campaign and
Work plan observations. Work plan filters, source rows and parsed item-detail labels also
use the catalogue. Named dynamic fields are validated before rendering.
Coordinator-provided Work plan source-detail chrome, source-state labels and
selected-task run-evidence labels also use the catalogue. Other coordinator
output still includes embedded English copy. The synthetic expanded and
right-aligned checks are local layout tests; a runtime language choice,
locale-specific number and date formatting, and full right-to-left qualification
remain open.

## Limits

- Preflight admits only the controlled-target boundary: one pod, exactly ten accepted outcomes,
  local-only publication, existing-login authentication, a dedicated branch and at least ten open
  sub-tasks. The readiness checks explain each condition.
- Supervised, exception-only and hands-off involvement modes are shown only from a mission charter
  the backend reports through `campaign-mission-status`. Without an admitted charter the modes are
  shown as unavailable with the admission command; the interface never admits, answers or revokes
  a mission itself. Mission contracts are deterministic local scope, not qualified no-intervention
  operation.
- Blocker clearance, trigger observation and integration or promotion approvals remain CLI-only
  because they bind operator-controlled evidence digests or remote effects.
- Reviewer finding text is not retained by the backend; the interface shows verdicts, correction
  rounds and evidence identities only.
- Report export runs through the public coordinator command in an isolated UI supervisor,
  so a stalled filesystem call does not block controls or window shutdown. The CLI writer
  retains a directory handle and checks the repository identity; another local process may
  still race a directory rename between checks. The export deadline is 30 seconds, but
  termination may wait on the filesystem. After timeout or an invalid receipt, inspect the
  destination and any `.codingmage-report-*.candidate` file before retrying. The local
  observation JSON is assembled on a separate CPU worker from a selection-bound snapshot;
  a changed observation makes that local view unavailable until reassembly. Export reads
  fresh coordinator records, so its contents can differ from the local view.
- The interface keeps private state under `$XDG_CONFIG_HOME/codingmage-ui` (or
  `~/.config/codingmage-ui`): recent configurations, per-configuration campaign memory,
  admissions, launch records with private stdout/stderr captures, and control ledgers.
