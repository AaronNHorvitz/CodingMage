# Sprint 36 Work plan final-report recovery

Task 36.3.2.2 remains open. Work plan presents a final-report freshness line,
an exact `campaign-report` Show-command and a read-only refresh for a selected
parallel campaign. Loading, failed, stale, never-requested and observed-absent
reports have distinct guidance. Pending, failed and stale observations hide
accepted badges while source checkboxes stay separate. The command is run by
the existing bounded backend worker; its response still requires the selected
campaign identity and the current completed status/branch/head before an
accepted badge is shown.

The focused disposable fake-provider parallel campaign test injects an
observed-absent report, a matching accepted report and then a report timeout.
It inspects the Work plan guidance and command preview, then requests a real
coordinator report again. This is local contract evidence, not a real-provider,
desktop accessibility or human trial. Other section-five states and controls
remain open.

## Verification disposition

The first focused run failed because the test-only campaign fixture was serial,
so it could not supply a parallel final report. The failed output is retained
privately. The corrected fixture has a validated one-pod parallel policy. Its
focused recovery test passed 1/1. The exact-source software-rendered native UI
all-target suite passed 149/149 across 14 targets. Strict workspace Clippy with
warnings denied passed. Formatting, architecture, documentation, no-write
verification inventory and whitespace checks passed. The inspected inventory
has 1,770 items and 825 explicit gaps: two new public UI methods, 16 moved
line-derived IDs, no removed semantic item and no change to applicability,
test mapping or gap count.

Python unittest ran 42 tests: 41 passed and only CM-R01.6's unchanged
eight-input source-bound package-freshness test failed. No digest was renewed.
The exact staged diff and additions are inspected for scope and private host or
credential material before commit. Independent review of this increment remains
pending; no human/live or release gate is claimed.
