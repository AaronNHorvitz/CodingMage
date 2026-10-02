# ADR 0083: Bind Guided Campaign Selection to Public Source Bytes

- **Status:** Accepted for local implementation; bounded independent review PASS
- **Date:** 2026-10-02
- **Decision owners:** CodingMage implementation

## Context

The ordinary native campaign picker already selects through the coordinator's
read-only `campaign-select` command. Guided campaign creation still checked
the write receipt by opening and parsing its destination on the interface
thread. That read could delay a frame and bypassed the public command boundary.
The version-one selection snapshot contained the canonical authority digest,
which does not bind the exact TOML bytes named by a write receipt.

## Decision

The held no-follow campaign loader returns the validated authority together
with the length and SHA-256 of the same source bytes. `campaign-select` emits
these fields in a closed version-two snapshot. The native decoder rejects
version-one and malformed version-two snapshots. Ordinary selection still
requires the exact selected path and observed repository identity.

The guided worker keeps the submitted form, diagnosis and three exact command
vectors fixed. After the coordinator confirms the candidate-byte write
receipt, the worker runs `campaign-select` and compares its source length and
digest with that receipt, then checks the resulting campaign, repository,
initial head and task-source identity. The interface adopts only that worker
result for the original request and current selection generation. It performs
no destination file read while accepting the response. Setup displays the
read-only selection command next to its inspection and write commands.

If the destination is replaced, linked, unreadable, malformed or differs from
the receipt, the worker refuses selection and the interface reports an
inspection action. A matching failed worker response withdraws a previously
selected campaign at the same destination. The source snapshot is an
observation at the public read boundary; it does not grant campaign admission
or remove the need for a fresh preflight before execution.

No new dependency, provider authority or runtime role is added. This is one
boundary increment of open Task 36.3.2.2; the complete state/depth catalogue,
human desktop trials and live qualification remain separate.

## Verification

Disposable real-process and native tests cover exact source binding,
unsupported and malformed snapshots, replacement after a write receipt,
normal guided selection and stale request handling. Exact results and limits
are recorded in `docs/evidence/sprint-36-guided-campaign-selection.md`.
