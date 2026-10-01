# ADR 0062: Guard CLI Report Publication Through a Private Stage

- **Status:** Accepted for local implementation; independent review pending
- **Date:** 2026-10-01
- **Decision owners:** CodingMage implementation

## Context

The public `codingmage report-export` command used a checked destination
directory but placed its candidate directly in that directory. Explicit
overwrite renamed the named candidate over a public leaf after a separate
check. A writer with access to the destination parent could replace either
name between those operations. This command will also be a useful publication
boundary for future guided Setup commands, which are still missing.

The native UI writer's earlier named-candidate and overwrite races were
reported by independent review. Decisions 0059 through 0061 record the
corrections. The exact-commit review of Decision 0061 returned a bounded PASS
with no open finding; that verdict covers the native UI writer, not this CLI
implementation.

## Decision

For the CLI writer, create a mode-0700 stage beneath the checked destination
descriptor. Admit the opened stage only when its held descriptor belongs to
the effective user, has owner read/write/search and no group or other access,
and still names the same inode. Write and validate the candidate inside the
held stage. Before publication, verify its inode and exact held-file bytes.

For a new leaf, create it with a descriptor-bound link that refuses an
existing name. For explicit overwrite of an existing regular leaf, exchange
the candidate and leaf atomically, verify the displaced inode, and restore a
changed leaf when the writer can prove that the public entry is still its own.
If the final state cannot be proven, return the stable
`codingmage.cli.uncertain_write` error and retain the private stage for
reconciliation. Never unlink a public leaf as error cleanup. Continue the
existing repository, mount, parent and destination checks.

The already pinned `nix` package provides the required Linux descriptor and
exchange calls; no new package or licence is added. The stage isolates a
writer with parent-directory access under a different effective user. A
hostile same-user process can still mutate its own files and is outside that
isolation promise.

## Consequences

An overwrite conflict can leave a private staging directory with displaced
material. The caller must inspect the destination and retained material
before retrying an uncertain write. A failed stage admission may leave an
orphaned original stage after another writer renames it; the CLI does not
guess the new name or remove a substituted directory. The separate guided
Setup CLI commands, asynchronous native worker and exact Show-command
disclosures remain open.

The native Reports client explains the new stable code with the destination
and staging-directory reconciliation action. The CLI troubleshooting guide
records the retained-directory naming pattern. Neither surface assumes a
write succeeded when final-state verification failed.

## Verification

See [the CLI report writer evidence](../evidence/sprint-36-cli-report-writer.md)
for focused and real-command tests, cumulative checks and independent review
disposition. This decision does not qualify installed desktops, live
providers, human-only checks or release.
