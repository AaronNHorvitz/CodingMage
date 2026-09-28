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

The initial implementation covers **Refresh diagnosis** (`doctor`) and **Run preflight**
(`campaign-preflight`). Every other control remains open under Sub-task 36.3.2.2 until its exact
backend or local-operation equivalent is implemented and tested. A command with a newly generated
request identity must show the identity actually submitted, rather than a placeholder.

## Verification and limits

A unit test checks shell quoting and visual-spoof refusal. Offscreen tests inspect the command
shown beside Refresh and Preflight using the same paths the real coordinator command receives;
viewing the preview must not modify the disposable target. These tests do not claim that all
controls now meet the rule, and the existing direct Git, run-record, configuration and local
state operations remain an open CLI boundary gap.
