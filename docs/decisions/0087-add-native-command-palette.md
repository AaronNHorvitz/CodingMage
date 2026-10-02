# ADR 0087: Add a Bounded Native Command Palette

- **Status:** Accepted for local implementation; bounded independent review pending
- **Date:** 2026-10-02
- **Decision owners:** CodingMage implementation

## Context

The native specification requires a keyboard command palette and access to
every destination and action. The shell already had Tab traversal and direct
screen shortcuts but no palette. The broader action catalogue and private
Setup recovery command equivalents remain open under Task 36.3.2.2.

## Decision

Add `Ctrl+K` search over a fixed list of the shell's nine destinations and
the existing read-only diagnosis refresh. The query filters first-party
labels only; repository or backend text cannot create an action. Arrow keys
select, Enter activates and Escape closes. Selection keys apply only when
search held focus before the current input frame, so Enter on a Show-command
disclosure only toggles that disclosure. The search field has a visible
AccessKit-associated label. The diagnosis entry uses the same argument
builder and bounded worker as the top-bar button, displays its exact command
in place, and is disabled when that preview is unavailable. Navigation works
offline. Opening the palette does not open a repository, run a coordinator
command, admit a campaign or edit task state.

This is one slice of the command surface. Remaining supported actions must
be added with their exact arguments and authority checks; private recovery
controls still need public command equivalents. The section-five screen
states and contextual depth remain open. No mode switch or new dependency is
introduced.

## Verification

See `docs/evidence/sprint-36-command-palette.md` for the focused keyboard,
real-coordinator and minimum-window software-rendering checks and the
cumulative verification disposition. Installed desktop, Orca, human trial,
performance-profile and live-provider qualification remain open.
