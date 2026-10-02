# ADR 0078: Bound Selected Configuration Reads to a Descriptor

- **Status:** Accepted; independent corrective re-review passed on `ef919df42469546333324ea5fc8b9124bf994d2e`
- **Date:** 2026-10-02
- **Decision owners:** CodingMage implementation

## Context

The independent review of the initial public `project-open` command found a
High race. That command read a bounded configuration snapshot and then called
the common config loader by pathname. The loader checked metadata before an
unbounded pathname read. File growth or a link substitution between those
steps could bypass the stated size and link limits. The original finding is
retained in the private independent report for the reviewed commit.

## Decision

The core config parser accepts an already-held byte slice and applies the
same schema and authority validation used by the ordinary file loader. It
rejects input above 1 MiB before decoding. `project-open` parses its first
bounded snapshot through this function and compares it with a second bounded
snapshot; it no longer invokes a second pathname loader. Both command
snapshot reads and the common loader open the selected file using a no-follow,
nonblocking descriptor and read at most the limit plus one byte. The command
rejects multiply linked files. The loader preserves its existing hard-link
admission policy but rejects link-count or descriptor identity changes. It also
checks that the selected name still refers to the opened descriptor after
reading. A changed selection is refused; no file content enters the error.

The `nix` dependency was already pinned in the workspace at 0.31.3 and used
by the CLI; the core crate now declares it for the descriptor open. Its MIT
licence is compatible with the existing dependency policy. The project-open
response schema and campaign authority remain unchanged. This corrects the
read bound; it does not make a changing filesystem an atomic transaction.

## Verification

Deterministic core hooks substitute a symbolic link or grow the file after
the first metadata observation. The link is refused before following the
outside name and the grown file returns the size error. A real-process CLI
test covers linked and oversized selected configuration, alongside the
existing valid selection and task-source cases. Exact commands and remaining
limits are in `docs/evidence/sprint-36-project-selection.md`. The fresh
independent exact-commit re-review returned PASS with no findings. Its scope
is the selected-configuration read correction; wider UI and external gates
remain open.
