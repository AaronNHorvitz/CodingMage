# ADR 0081: Public Campaign Selection Snapshot

- **Status:** Accepted for implementation; independent re-review pending
- **Date:** 2026-10-02
- **Decision owners:** CodingMage implementation

## Context

The native campaign picker listed files through the coordinator, then selected
one on the render thread. Independent review of the directory-picker commit
found that a file replaced after listing could be followed by the campaign
loader. ADR 0080 secures the shared loader, but native selection still needs a
public read boundary and current-project response binding.

## Decision

`codingmage campaign-select --campaign <ABSOLUTE_FILE>` returns a bounded,
version-one read-only JSON snapshot of the exact validated campaign authority,
its selected path and canonical authority digest. The command uses the held
no-follow loader from ADR 0080. It never admits or runs the campaign. Invalid
files and oversized output produce stable content-free errors.

Native selection submits this command through its existing bounded worker
only after the opened repository has a current coordinator diagnosis. The
request is bound to the selected path, generation, configuration, observed
repository identity and unique request ID. The decoder rejects unsupported
versions, unknown fields, malformed or mismatched paths and digests, and
foreign repository path or identity. Changing selection, project or entered
path before the response prevents adoption. Remembered campaign paths wait for
diagnosis before the same read. An attempt made with stale diagnosis clears
the prior in-memory campaign. If a diagnosis refresh was already running, its
replacement is queued before the snapshot and a failed refresh prevents
adoption. The UI shows loading, failure and the exact
Show-command invocation. Status observation begins only after a validated
selection response. Directory entries remain navigation hints, never authority.

Guided campaign creation still uses its separate exact write receipt and
bounded destination-byte check. Moving that recovery read off the render
thread belongs to the remaining Task 36.3.2.2 work; this decision does not
claim that full task complete.

No new package, grant, provider, runtime role or execution scheduler is added.
The command uses existing campaign, JSON and worker dependencies.

## Verification

Real-process and native tests cover a normal snapshot, exact-path and digest
validation, malformed/unknown response refusal, a file replaced with a link
after a listing, wrong request identity, stale generation and no downstream
status on refusal. Exact results and retained limits are in
`docs/evidence/sprint-36-campaign-selection-read.md`.
