# Sprint 36 final-report payload and outcome binding

Task 36.3.2.2 remains open. The UI now checks a `campaign-report` payload
against the selected campaign ID, repository ID, starting commit and branch
prefix, even when the request itself has a current binding. A foreign payload
is a contract failure. Work plan shows a report-derived accepted outcome only
with a live, completed status for the same campaign, exact branch and final
head, and without a pending or failed report refresh. It keeps source claims
separate and does not interpret a retained final report as current status.

A disposable repository with fake providers exercises one real coordinator
unit, then injects synthetic matching, foreign-identity, foreign-branch,
wrong-head and failed-status responses at the client boundary. These injected
responses are contract tests, not real-provider or independent acceptance.
The broader section-five state/depth/action catalogue, human desktop checks,
CM-R01.6 source-bound package renewal and release remain open.
Work plan still needs an in-context report-specific refresh and failure
explanation; the shell's F5 campaign refresh is the current recovery path.

## Verification disposition

The first focused run failed because the synthetic report declared schema 1
while the runtime contract declares schema 2. A second run failed a test-only
assumption that the one-task disposable campaign would remain paused; it had
completed. Both failures are retained in the private test record. The corrected
test uses the actual completed status and passed 1/1. The software-rendered
native UI all-target suite passed 148/148 tests across 14 targets before a
test-only fixture extraction and a null-report assertion. The final formatted
source then passed the focused report-binding test 1/1 and strict workspace
Clippy with warnings denied. No production behavior changed after the
all-target run.

Python unittest ran 42 tests: 41 passed, and only the unchanged CM-R01.6
eight-input source-bound evidence-freshness check failed. No binding digest
was renewed. Formatting, documentation, architecture, no-write verification
inventory and whitespace checks passed. The inspected generator reports 1,768
items and 825 explicit gaps: one new public UI handler, nine existing
line-derived IDs moved, and no removed semantic item, applicability, heuristic
test mapping or gap-count change. The exact staged diff and additions are
inspected for scope and private host or credential material before commit.
Independent exact-commit review of this increment is pending; no human/live
or release gate is claimed.
