# CodingMage Native UI Specification

Status: binding specification under
[Decision 0020](../decisions/0020-complete-native-ui-specification.md), 2026-09-27. It is
required scope, not a description of working features. The [PRD](../../PRD.md) owns the
CM-UI requirement IDs and [TASKS](../../TASKS.md) is the only completion ledger. Current
behaviour and limits are recorded in the [native workspace guide](../operations/native-ui.md)
and the Sprint 36 evidence.

## 1. One interface for everyone

CodingMage serves a repository owner who may be new to coding agents and an experienced
developer who wants every detail. There is one interface for both, with no beginner, expert or
advanced mode. Every screen is simple by default and offers more depth in context, next to the
thing it describes. Supervised, exception-only and hands-off are campaign authority settings
under CM-TEAM-002, not UI modes, and never change what a screen can show.

## 2. Architecture rules

- **Toolkit.** egui through eframe with AccessKit (Decision 0015). No embedded browser.
- **Public boundary only.** Every operation with repository, state or process authority runs the
  sibling `codingmage` executable, as Decision 0016 requires. The UI never opens coordinator
  journals, locks or provider state. A need the command boundary cannot meet is a boundary gap
  to fix, never a UI shortcut.
- **Coordinator state is authoritative.** The UI keeps presentation state only and shows what
  the coordinator's machine-readable output reports.
- **Never block the UI thread.** Subprocesses, file reads and heavy formatting run off the UI
  thread. Rendering repaints only on input, backend results or active animation.

## 3. Application shell

- **Navigation.** Labelled destinations: Workspace, Work plan, Team activity, Blockers, Changes
  and evidence, Reports, Configuration, Settings, and Help.
- **Status bar.** Always visible: repository, campaign identity and state, involvement setting,
  active pods, current gate and the last backend update time. Stale data is marked stale.
- **Activity log.** One chronological, filterable record of campaign events, controls, gates,
  failures and recoveries from real backend records.
- **Command palette and shortcuts.** Every action and destination is reachable by keyboard.
- **No dead ends.** Every screen offers a way back, a next step and help.

## 4. Screen inventory

Each screen implements every applicable state in section 5.

| Screen | Purpose and primary actions |
|---|---|
| First run | Locate the `codingmage` executable, run `doctor`, explain provider sign-in and storage, open a repository; nothing starts automatically |
| Workspace | Repository overview, current campaign, what needs attention and recent outcomes |
| Work plan | Searchable, filterable tasks with dependencies, source locations and distinct source-checkbox versus verified-outcome states |
| Task detail | One task's source text, acceptance criteria, dependencies, packets, attempts, review and test history |
| Team activity | Actual lead, pod, reviewer and QA activity with bounded output and timings |
| Blockers | Exact blockers with explanation, affected work and the action that would clear each one |
| Changes and evidence | Candidate diffs, exact-commit review findings, test results and gate outcomes; missing evidence is shown as missing |
| Reports | Inspectable and exportable outcome and blocker reports with privacy and overwrite safeguards |
| Configuration | Guided target, provider and model profiles, limits, involvement and delivery policy, readiness and preflight |
| Campaign controls | Admission, start, pause, resume, stop after unit, cancel, revoke, detach and reconnect |
| Settings | Appearance, accessibility, language, storage locations and defaults |
| Help and About | Contextual help, glossary, shortcuts, licences and attributions, manual redacted diagnostics |

## 5. Truthful state model

- **Campaign and task states** show exactly what the coordinator reports: not started,
  admitted, running, paused, stopping, completed, accepted, blocked, deferred, failed,
  cancelled, revoked, or interrupted and recoverable.
- **Screen states** handle empty, loading, partial, stale, disconnected, executable missing,
  provider unavailable, authentication required, permission denied and malformed backend
  output, each with specific wording and a recovery action.
- **Never optimistic.** Success appears only after the backend records it. A source checkbox is
  never shown as verified completion. Unknown token usage stays unknown.
- **Controls are honest.** A control that the backend does not yet support is absent or
  explained, never a working-looking button.

## 6. Simple by default

- **Start from the objective.** First run and Workspace lead with "what should the team do?"
  and one recommended next step.
- **Plain language.** No unexplained jargon on any screen; technical terms link to the glossary.
- **One clear next step.** Each screen has one primary action; others stay visible but quieter.
- **Guided recovery.** Errors say what happened, why and what to do, with a button for the fix
  where one exists: sign in, rerun preflight, free resources, resume or reconnect.
- **Safe by default.** Opening or browsing never starts agents, edits tasks or grants authority.

## 7. Depth in context

Detail lives where it is relevant, collapsed until opened. Opening detail on one screen does
not change any other screen.

| Where | Depth available in place |
|---|---|
| Every task | Packets, attempts, prompts sent with secrets redacted, provider and model, timings and exact commit identities |
| Every control | A "Show command" link giving the exact `codingmage` command the UI will run |
| Every review and test | Full findings, logs with bounded previews, and the exact commit and test command |
| Configuration | The validated configuration file view, schema version and every default in effect |
| Team activity | Per-pod process, resource and time limits and their observed usage where available |
| Help and About | Copyable diagnostics that never include repository content, prompts or credentials |

Every UI action has a command-line equivalent, shown by "Show command".

## 8. Content safety

- **Inert rendering.** Repository files, task text, model output, review findings and logs are
  untrusted. They render as text or a restricted Markdown subset with no scripts, no remote
  resource loading and no automatic execution.
- **Links.** Links in content show their full destination and open only after confirmation.
- **Trusted chrome.** Campaign controls, authority changes and destructive confirmations appear
  only in UI chrome that content cannot imitate. Repository text and model output never grant
  permission.
- **Clipboard and files.** Copying, saving and exporting happen only on explicit user action.
- **No secrets.** Credentials and tokens are never displayed, logged or placed in diagnostics.

## 9. Design system

One token source for colour, typography, spacing and radius; light, dark and high-contrast
themes following the system preference; shared components with one behaviour and accessibility
contract; status never shown by colour alone; every icon-only control labelled; reduced-motion
support.

## 10. Accessibility

The WCAG 2.2 level AA success criteria, applied to a native application: contrast of at least
4.5:1 for text and 3:1 for controls and focus indicators; complete keyboard operation with visible
focus and no traps; AccessKit names, roles, states and polite announcements of asynchronous
changes, verified with Orca; usable from 100% to 200% scaling and at a minimum window of 1024 by
640 logical pixels.

## 11. Device profiles and responsiveness

Task 36.3.1 pinned target hardware and froze the full numerical matrix and trial scoring before
measurement in [the Sprint 36.3 baseline](../evidence/sprint-36-ui-specification-baseline.md).
Devices that are unavailable remain unqualified. The summary budgets are:

| Profile | Description |
|---|---|
| P1 | Reference Linux workstation |
| P2 | Mainstream Linux laptop, 16 GB, integrated graphics, 125% to 150% scaling |
| P3 | Low-memory Linux computer, 8 GB, no usable GPU |
| P4 | High-DPI display at 200% scaling |
| P5 | Windows 11 x64, when that platform lane opens |

| Budget | Frozen target |
|---|---|
| First interactive frame | Within 2 s on P1, 3 s on P2/P4 and 4 s on P3/P5 |
| Input to visible response | p95 at most 100 ms |
| Frame time during live activity and scrolling | p95 at most 16.7 ms on P1, 25 ms on P2 and 33 ms on P3/P4/P5 |
| Idle cost | At most 1% of one CPU core and 300 MB resident memory, excluding the coordinator and providers |
| Scale | 10,000 tasks, 100,000 activity events and 1,000 blockers scroll and filter within the budgets above |

## 12. Localization, help and recovery

All user-facing text lives in externalized message catalogues. Dates, times, numbers and
durations follow the locale. Layouts tolerate text 40% longer than English and right-to-left
scripts. Every error names its cause, its effect on the campaign and the next step. Help and the
glossary work offline. About lists every component licence and attribution.

## 13. Verification program

- **Automated.** Per screen and state: accessibility-tree assertions, keyboard traversal, state
  rendering for every section 5 state with fake backends clearly labelled, and screenshot
  comparisons across themes, 100% and 200% scaling and the minimum window.
- **Fault injection.** Missing executable, provider sign-out, killed subprocess, malformed and
  oversized backend output, full disk and slow responses, each showing the specified state.
- **Performance.** The section 11 budgets on each declared profile, with method and hardware.
- **Human trials.** At least eight representative first-time owners per round, each core
  workflow completed unaided by at least seven, with no open severity-1 finding at the gate;
  and a round of experienced developers using only in-app help for the in-context depth.
  Trials use the real backend and disposable repositories; fake-provider runs never count.
- **Evidence.** Each result records commit, build, profile, method and failures. Nobody approves
  their own review.
