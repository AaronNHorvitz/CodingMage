# ADR 0079: Public Directory Snapshots for Native Pickers

- **Status:** Accepted for implementation; independent review pending
- **Date:** 2026-10-02
- **Decision owners:** CodingMage implementation

## Context

The native Open, Setup target and campaign file pickers previously enumerated
directories on the render thread. This could stall the window on a slow
filesystem and bypassed the coordinator boundary required for repository
observations. A picker list is navigation data, not authorization to read a
configuration, select a campaign or start work.

## Decision

The existing `codingmage` executable exposes a read-only
`directory-list --directory <ABSOLUTE_DIR>` command with a version-one JSON
snapshot. It lists at most 2,000 visible UTF-8 entries after scanning at most
4,096 directory entries, labels directory, file and symbolic-link kinds, and
marks truncated results. It refuses a selected symbolic link and holds a
no-follow, nonblocking directory descriptor while enumerating through Linux procfs. The
selected path must still name that descriptor after enumeration. The response
is capped at 1 MiB, including the CLI newline. A changed or unavailable
selection fails with a stable content-free CLI code. Linux procfs is a runtime
prerequisite for this command; its absence is an explicit listing failure.

All three native pickers submit this command to the existing bounded worker
with an exact path, generation, selection binding, deadline and globally unique
request identity. The interface accepts a version-one response only for the
current picker request and exact directory. It derives child paths from safe
single-component names, renders names with the existing control-safe bounded
list label, disables symbolic links, and retains no old rows on a
failure or navigation. Setup enables "Use this target" only after a successful
current listing. Each picker shows loading, failure, truncation, Refresh and
the exact Show-command invocation. Opening or selecting a listed file still
uses the separate authoritative coordinator command and its own validation.

This command observes a moving filesystem. It does not promise an atomic
snapshot or grant authority from the listing. A separate recovery path for a
detached guided configuration write remains to be moved behind a public
coordinator read.

No dependency was added. The CLI's already pinned `nix` 0.31.3 (MIT) supports
the held no-follow descriptor. The native UI uses its existing JSON and worker
dependencies.

## Verification

Real-process tests cover deterministic small-directory output, no repository
write, hidden entries, symbolic-link refusal, malformed arguments and bounded
truncation. Native tests cover filtering, unsafe or cross-directory response
refusal, three independent picker identities, late results and changed
selection generation. The exact verification disposition is in
`docs/evidence/sprint-36-directory-browser.md`.
