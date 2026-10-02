# ADR 0077: Read Project Selection Through the Coordinator

- **Status:** Accepted for the native implementation; independent review pending
- **Date:** 2026-10-02
- **Decision owners:** CodingMage implementation

## Context

The native Open control called `Project::open` on the render thread. That
validated a configuration and read the task source directly in the UI process.
The current native specification requires repository reads through the sibling
`codingmage` command and forbids blocking file work on the render thread. The
existing `doctor` output is intentionally content-minimized and cannot supply
the configuration form or work-plan items.

## Decision

Add the read-only `codingmage project-open --config <ABSOLUTE_FILE>` command.
Its version-one JSON response contains the exact selected configuration path,
a SHA-256 of the selected configuration bytes, the validated configuration and
one task-plan result: `loaded`, `invalid`, `unavailable` or
`projection_too_large`. An invalid result retains the strict parser's stable
content-free reason code. A loaded result contains the strict parser's plan,
source digest and byte count. Configuration validation uses the same schema
and authority validator as the loader on held bytes, then compares bounded
snapshots read before and after validation. Task source bytes are read with
a size bound, a resolved in-repository path check
and linked-inode refusal. A parsed response larger than the native 8 MiB
output limit becomes an explicit plan failure; the CLI reserves one byte for
its output newline. A valid configuration remains openable. The command grants
no campaign authority and starts no agent.

The native Open control sends this exact command through its bounded worker.
Its response must match the requested path, supported schema, digest grammar,
plan identity and current selection generation. Until that response arrives,
the UI shows an opening state; it does not report success. Malformed, unknown
or cross-project output fails visibly. The control exposes the exact command
in context. The existing private recent-list write occurs only after a valid
response. If a later `doctor` observation reports a different task-source
digest, the native client clears the displayed plan and marks it stale until
the configuration is reopened. A doctor target-path fingerprint that differs
from the opened configuration is an authority mismatch: the UI refuses that
diagnosis and also clears the plan.

This increment covers the Open and recent-configuration selection action.
The Setup directory picker and guided-configuration recovery still read files
in the UI process; those are separate open parts of Task 36.3.2.2. The
existing direct library parser remains for isolated tests and recovery until
that boundary is migrated. Decision 0078 records the later read-race
correction and dependency change; the original read path is no longer used.

## Verification

The exact commands, source revision, outcomes and limits are recorded in
`docs/evidence/sprint-36-project-selection.md`. The initial exact-commit
review found a High read race; Decision 0078 addresses it and requires
re-review. This decision does not close the native state/depth
catalogue, installed desktop, Orca, human trials or live-provider gates.
