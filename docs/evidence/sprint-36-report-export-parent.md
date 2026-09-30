# Sprint 36 report export destination correction

Task 36.3.2.2 remains open. The native Reports screen used to compare the export
destination and repository as written path prefixes. An absolute path containing `..`
or an outside symbolic-link parent could resolve to a file inside the target repository.
Decision 0037 requires an existing parent and resolves it with the repository before any
report bytes are written. An in-repository parent, including one reached through a link,
is refused. The original destination symlink and overwrite checks remain in place.

The focused unit test covers linked and traversal parents, a missing parent that is not
created, an ordinary outside export, and the absence of a target report file. The disposable
real-coordinator Reports workflow also selects an outside link to its target and checks
the visible refusal and absence of a target report before a permitted outside export.
These are static-path and fake-provider checks. They do not exercise a malicious process
changing directory links between validation and write. Export still writes on the UI
thread and has no exact `codingmage` command equivalent; both remain open against the
native UI specification's command and nonblocking boundaries. No source checkbox,
coordinator status or review authority is changed by the export.

## Verification disposition

The focused unit test passed 1/1 and the focused disposable real-coordinator Reports
workflow passed 1/1. The exact-source native UI suite passed 120/120 across 14 targets,
including the existing setup-to-outcome, recovery, keyboard and software-rendered
verification fixtures. A repeated owner restart interrupted an earlier combined test
process during the last Execution case; its partial log is retained privately, and the
remaining targets were rerun to completion under the shared build reservation. Formatting,
documentation, architecture, no-write inventory and diff whitespace checks pass. The
generated inventory remains at 1,738 items and 825 explicit gaps; eight line-derived IDs
moved in `report.rs`, with no changed semantic surface, test mapping or gap. Strict workspace
Clippy passed with warnings denied. Python unittest ran 42 tests: 41 passed and only the
retained CM-R01.6 evidence-binding check failed with the same eight source-bound input
drifts. No digest was renewed. Independent exact-commit review is pending.
No installed desktop, Orca, frozen performance, human, live-provider, release or delivery
qualification is claimed.
