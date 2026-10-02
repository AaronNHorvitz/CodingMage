# ADR 0076: Supervise Guided Configuration After Window Close

- **Status:** Accepted for local native implementation; independent review pending
- **Date:** 2026-10-02
- **Decision owners:** CodingMage implementation

## Context

The native configuration form previously called its library writer on the UI
thread. Decision 0074 introduced a public coordinator command for the same
write, and Decision 0075 admitted one checked missing workspace parent. A
configuration can already be published when a user changes selection or
closes the window. Losing the command's receipt at that point leaves an
uncertain result that a new window must not silently retry or call successful.

## Decision

The form freezes its candidate TOML and exact displayed argument vector in a
create-only owner-private intent before queueing the command. A separate
native helper validates the intent digest, holds an exclusive intent lock,
runs `setup-write-config` with the candidate on private standard input, and
records a bounded terminal outcome. Once started, the helper survives the
observer window. The helper checks the public receipt against the submitted
byte count and digest, the held named output bytes, and the repository identity
and HEAD observed from the submitted configuration. A missing, malformed or
changed result is uncertain; it cannot authorize another write. Recovery
shows the frozen command and destination, offers an explicit check, and
loads the selected project only from a configuration whose parsed value
matches the submitted candidate while the named bytes still match the
receipt. It clears the intent only after that check; an unavailable root or
changed file retains recovery state. Manual clearing of the local notice
requires the helper to release its lock and the user to inspect the
destination. The UI never automatically replays an unresolved write.

The public coordinator remains the only writer and authority. Opening Setup,
checking recovery and showing the command start no campaign. No dependency,
licence, provider or publication policy changes.

## Verification

Disposable native real-process tests exercise first publication with a
previously missing private workspace, form refusal and no-overwrite, a held
public receipt across window close and reopen, and selection change plus
destination replacement after public publication. The exact check results
and remaining state/depth work are recorded in the adjacent Setup evidence.
