# ADR 0075: Create a Checked Parent for Guided Configuration

- **Status:** Accepted for local backend correction; independent review pending
- **Date:** 2026-10-02
- **Decision owners:** CodingMage implementation

## Context

The guided native form proposes a new workspace directory next to a repository.
Decision 0074's public `setup-write-config` command required its output parent
to exist. The interface cannot make that directory itself under Decision 0016's
public command boundary, and requiring manual creation would leave the first-run
action different from the form it presents.

## Decision

After the exact submitted TOML parses and names the selected repository, the
coordinator may create one missing configuration parent with owner-private
permissions. It first validates the prospective parent as an external
destination through the existing guarded writer, then creates that direct
child. It validates the resulting configuration destination before admitting
the scratch and state roots. Existing linked or non-directory parents are
refused. A malformed candidate creates no parent. A later authority failure
may leave an empty private parent and roots; the operator inspects them rather
than the interface assuming it owns cleanup.

This correction changes no campaign authority, provider behavior, dependency,
licence or publication policy. The native form still needs the durable helper
integration and exact receipt recovery.

## Verification

The disposable real-process Setup configuration tests cover first publication
with a missing parent and private mode, malformed input with no directory
effect, and refusal of a missing parent inside the selected repository. Exact
commands and limitations are recorded in the adjacent evidence note.
