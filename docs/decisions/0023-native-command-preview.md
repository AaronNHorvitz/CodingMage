# Decision 0023: Exact Native Command Previews

- **Status:** Accepted for incremental native UI implementation
- **Date:** 2026-09-27
- **Decision owners:** CodingMage implementation worker under the owner's delegated UI scope
- **Supersedes:** None
- **Superseded by:** None

## Context

The native UI specification requires an in-context **Show command** affordance for every
control. An independently written example can drift from the command the UI sends, especially
when an authorization path or idempotent request identity changes. A command preview also must
not execute, leak into a screenshot by default or visually hide a dangerous argument.

## Decision

Each backend action builds its exact argument vector once and uses that vector for both the
bounded worker request and a collapsed command preview beside the action. The preview includes
the resolved sibling executable and POSIX quoting that preserves argument boundaries. It refuses
to display a command when an argument or executable path cannot be represented as UTF-8
or contains an invisible control or direction character. The matching action is disabled in
that case, so an executable action always has an exact preview. Showing a command never submits
a request or copies text. The control remains governed by the existing backend authority.

After independent review of the first implementation, the display check uses a conservative
character policy: printable ASCII plus ordinary non-ASCII letters and numbers, excluding known
blank letters. Other Unicode marks, formatting characters, unusual whitespace and symbols make
the exact preview unavailable and disable the matching action. This can refuse some legitimate
paths; it avoids showing an apparently complete command while hiding an argument character.

An independent review of the campaign-control increment found that Rust's alphanumeric
predicate includes some combining marks. The display check now uses the pinned
`unicode-general-category` 1.1.0 crate (Apache-2.0; crates.io registry origin already present
transitively in the workspace lockfile) to admit only letter and number general categories.
Marks are refused regardless of the language-specific alphabetic predicate. The explicit blank
letter exclusions still apply. This changes only preview eligibility and its matching action;
the coordinator's authority and command arguments are unchanged.

The initial implementation covered **Refresh diagnosis** (`doctor`) and **Run preflight**
(`campaign-preflight`). At that checkpoint, other controls remained open under Sub-task 36.3.2.2
until each exact backend or local-operation equivalent was implemented and tested. A command
with a newly generated request identity must show the identity actually submitted, rather than a
placeholder.

The later campaign-control increment prepares a display-only request identity for each action.
Preparation writes no intent. Submission records that exact identity in the durable UI ledger
and sends the same argument vector to the coordinator. A retry with an unknown outcome previews
the prior idempotency identity. A changed campaign authority or a different action cannot reuse
it. The coordinator start preview uses the same argument builder as the detached process launch;
both refuse a path whose exact command cannot be displayed safely.

## Verification and limits

A unit test checks shell quoting and visual-spoof refusal. Offscreen tests inspect the command
shown beside Refresh and Preflight using the same paths the real coordinator command receives;
viewing the preview must not modify the disposable target. These tests do not claim that all
controls meet the rule. Later Decisions 0024 through 0026 moved the campaign-head task, change
and run-record observations through the coordinator. Configuration and other local state
operations still need their command equivalents under Task 36.3.2.2.
