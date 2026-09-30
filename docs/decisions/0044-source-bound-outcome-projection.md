# Decision 0044: Source-Bound CLI Outcome Report and Guarded Export

- **Status:** Accepted for an incremental CLI report boundary
- **Date:** 2026-09-30
- **Decision owners:** CodingMage implementation lane under Decision 0020
- **Supersedes:** None
- **Superseded by:** None

## Context

The native Reports view assembles coordinator observations and writes its export through
an isolated interface helper. The helper accepts an interface-supplied document and has no
public `codingmage` command equivalent. A public command must assemble records from the
coordinator and authorized repository itself. Accepting an arbitrary document supplied by
the interface would turn presentation data into report authority.

## Decision

Add a read-only `campaign-outcome-report` command as the source-bound assembly step. It
loads the verified campaign specification and authorized repository identity, reads the
content-minimized status, blocker, final-report and run projections, and reads changes at
the exact coordinator head. It rechecks durable status after assembly and refuses a
changed observation. The document marks absent records as absent and omits changed-file
paths. It does not claim interface-only admission or last-invocation observations.

The command returns a bounded version-one JSON document. An explicit `report-export`
command writes the same freshly assembled document to an absolute file outside the target
repository. Changed-file paths require an explicit `--include-paths true`; replacing an
existing regular file requires `--overwrite true`. Its writer holds a checked Linux
directory descriptor, stages private bytes and checks parent/repository identity before
and after publication. No UI-supplied JSON is accepted by this command.

The CLI adds a direct dependency on the workspace-pinned `nix` 0.31.3 crate for
directory flags. Its MIT licence was recorded for the native writer in Decision 0038;
no new crate version, provider call, credential, acceptance authority or release outcome
is added. The existing native report and writer remain in place until exact interface
delegation is separately implemented and verified.

## Consequences

This increment makes a coordinator-owned report projection inspectable and exportable by
a shell user. The native interface does not yet invoke the new command, and its inline
schema-three report is not identical to this CLI schema-one document. Task 36.3.2.2,
native Show-command parity, full state/depth catalogue and human qualification remain
open. The document's bounded change and run projections retain their truncation
indicators; they are not a full evidence archive. The guarded writer and the native
writer currently repeat the same first-party containment checks; unifying that code is
follow-up work before treating the two paths as one contract.
Decision 0045 adds a same-mount restriction to both writers after independent
review found a bind-mount alias outside the path-only containment check.

## Verification

Disposable repository tests exercise absence before a campaign, bound records after a
real coordinator invocation with a fake role provider, omission and explicit inclusion
of repository paths, overwrite consent, linked and in-repository destination refusal,
and private objective omission. Writer tests inject parent replacement, movement and
competing leaf creation. The precise check results and limitations are recorded with
this increment's Sprint 36 evidence.
