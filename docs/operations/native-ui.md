# Native Linux Desktop Workspace

## Status

`codingmage-ui` is the native Linux desktop workspace over the existing coordinator. It is
implemented locally with deterministic, display-less tests against the real `codingmage` binary
and fake providers (Sprint 36 in [TASKS.md](../../TASKS.md)). It has not been verified on a real
desktop session, with a screen reader, from a clean installation, or with real providers; those
items are listed in [the human-only register](../evidence/sprint-36-human-only.md). Independent
review of the interface is pending.

## What It Is

- A client of the `codingmage` executable installed next to it (Decision 0016). Every action
  with repository, state or process authority runs that binary; the interface parses its JSON and
  stable error codes and never links the runtime.
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

## Screens

| Screen | Content | Source |
| --- | --- | --- |
| Overview | Configuration summary, repository diagnosis (identity, head, branch, cleanliness, denied capabilities), task-source counts | `codingmage doctor`, `codingmage-core`, `codingmage-plan` |
| Work plan | Searchable, filterable plan with dependencies, source anchors, readiness from the source, disabled source checkboxes and the coordinator overlay (completed at campaign head, accepted, active, blocked, deferred, human decision, unknown) | task parser, `campaign-status`, read-only `git show` of the campaign head |
| Campaign | Campaign authority, binding drift, readiness checks, preflight, admission, coordinator process state, controls, durable status, holds, utilization, roles the backend reports, unavailable involvement modes | `campaign-preflight`, `campaign-status`, `campaign-explain-blocker`, `campaign-control`, `/proc` |
| Changes and reviews | Delivery boundary, coordinator commits and changed files, per-run verdicts and gate evidence, journaled phases, bounded activity | read-only `git log`/`git diff --numstat`, run checkpoints and journals |
| Reports | Outcome and blocker reports with export | the observations above |
| Setup | Open or create a configuration, write the owner's authorization record, author a campaign, import and export | `codingmage-core`, `codingmage-campaign`, `codingmage init` |

## Workflow

1. Open a configuration (path, recent list or browser), or choose a repository directory in
   Setup and write a deny-first configuration; the existing loader validates it first.
2. Write the owner's authorization record outside the repository and author a campaign; the
   repository identity, head and task-source digest come from the live diagnosis.
3. On the Campaign screen, review the local readiness checks, run preflight, and admit the
   campaign by typing the first twelve characters of the report digest. Admission records your
   review; it grants nothing.
4. Start the coordinator. It runs `codingmage campaign` in its own process group; the interface
   shows its pid, activity lines and outcome, and you may close the window at any time.
5. Pause, resume, stop after the current unit or cancel through `campaign-control`. Cancel
   needs a second press. Resume records the intent; press Start again to continue a paused
   campaign.
6. Inspect changes, review and test records, and export a report to a path outside the
   repository. Repository file paths are excluded unless you opt in.

## Keyboard

- `Ctrl+1` to `Ctrl+6` switch screens; `Tab` and `Shift+Tab` move focus; `Space` or `Enter`
  activate; `F5` refreshes the diagnosis and campaign observations.

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
- The interface keeps private state under `$XDG_CONFIG_HOME/codingmage-ui` (or
  `~/.config/codingmage-ui`): recent configurations, per-configuration campaign memory,
  admissions, launch records with private stdout/stderr captures, and control ledgers.
