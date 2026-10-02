# ADR 0080: Hold the Campaign Specification During Validation

- **Status:** Accepted for implementation; independent review pending
- **Date:** 2026-10-02
- **Decision owners:** CodingMage implementation

## Context

The core campaign loader checked that a selected path was an ordinary file,
then opened it by name. A concurrent replacement between those operations
could redirect the read through a symbolic link. An independent review of the
native directory picker identified this path, including its use during native
campaign selection. A directory listing cannot authorize a later file read.

## Decision

The core loader now opens the selected absolute path with a held, nonblocking,
no-follow descriptor. It compares the opened file's device and inode with the
initial ordinary-file inspection and with a final inspection after the bounded
read. It refuses links, multiply linked files, nonregular files, changed names,
oversized bytes and invalid campaign specifications. The parsed campaign is
derived only from the held bytes. The existing stable invalid-spec error does
not disclose the path or contents.

This closes the file replacement gap for all callers of the shared loader. A
separate native selection command and worker binding remain necessary to remove
the render-thread read and to bind the UI response to the current selection.

The crate adds an edge to the workspace's existing exact `nix` 0.31.3
dependency (MIT). No new package, effect authority, credential or campaign
execution path is introduced.

## Verification

A deterministic test replaces an ordinary selected file with a symbolic link
between inspection and open; the loader refuses it. The normal exact-byte
loader test remains. The batch verification result is recorded in
`docs/evidence/sprint-36-campaign-selection-read.md`.
