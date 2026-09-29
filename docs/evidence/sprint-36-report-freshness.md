# Sprint 36 report observation freshness increment

The Reports screen now names the observation freshness of campaign status, blocker explanation,
final report, changes and run records. Loading, never-requested, stale or failed sources get a
visible warning. The same statement is retained in the exported report's `limits` list at
assembly time. A report built from a retained observation therefore does not silently look
current after a failed refresh. This uses the existing `Observed` state and changes neither
coordinator authority nor the report schema.

The bound disposable-repository test first observes a real coordinator status, injects a failed
refresh on that exact selection, and checks the screen warning, report limitation, exported JSON
and unchanged target tree. The test is a local fixture, not live-provider qualification.

## Verification disposition

The focused report/export test passed 1/1; strict workspace Clippy passed; the software-rendered
native UI all-target suite passed 98/98. Python unittest passed 41/42, with only CM-R01.6's
eight source-bound input drifts. Exact-commit independent review remains pending. Task 36.3.2.2
stays open for the complete section-5 state catalogue and contextual depth on every screen.
