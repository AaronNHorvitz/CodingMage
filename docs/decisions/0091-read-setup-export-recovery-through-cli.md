# ADR 0091: Read Setup Export Recovery Through the Public CLI

- **Status:** Accepted for local implementation; bounded exact-commit review passed
- **Date:** 2026-10-02
- **Decision owners:** CodingMage implementation

## Context

The native Setup export already invokes the public `setup-export-copy` command
through a detached helper. After window close, its read-only outcome control
still inspected the helper's private terminal record and destination inside
the UI worker. Native UI specification sections 2 and 7 require the sibling
`codingmage` executable to own this state read and the UI to show its exact
command. The explicit manual notice-clear action remains a separate gap.

## Decision

Add `codingmage setup-export-recover` with the exact private intent path and
digest, request ID, selected configuration and diagnosed repository ID. The
command reads version-one intent and terminal records with no-follow held
descriptors and size limits, requires exact identities and a successful
content-minimized export receipt, and verifies the named destination's byte
count and SHA-256. It changes no file, campaign state or authority. A missing,
failed, malformed, foreign or changed outcome has no success response. Its
version-one JSON result contains only bound identities, destination digest
and a verified flag; it never includes the exported document.

The native control sends the same argument vector shown in its in-context
Show-command disclosure through the existing bounded worker. Unsafe or
unrepresentable previews disable the control. The UI checks the full
response schema and identities before requesting the existing second,
verified private cleanup, which rechecks the terminal record and destination.
The command never clears the notice itself. Other private Setup recovery and
clear controls, complete section-five screen states and contextual depth
remain open under Task 36.3.2.2.

The independent read-only review of commit
`3d09708f66de247bf57d212dac8e0caefd6fab23` returned `Verdict: PASS`
with no findings in this bounded scope. Decision 0092 owns the later public
clear action and does not change this review's scope.

This uses the workspace-pinned `serde` dependency directly in the CLI for
closed record decoding. `serde` is available under MIT or Apache-2.0;
no version, product licence, provider or runtime authority changes.

## Verification

See `docs/evidence/sprint-36-setup-export-recovery-command.md`. Synthetic
CLI fixtures cover success, stale or foreign identities, changed destination,
malformed terminal data and a linked result. A native response test covers
the exact command preview, bound success, a foreign response, unknown fields
and malformed JSON. These local checks do not qualify installed desktop,
screen-reader, human or live-provider acceptance.
