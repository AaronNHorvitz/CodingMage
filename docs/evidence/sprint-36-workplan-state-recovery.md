# Sprint 36 Work plan state and recovery increment

Task 36.3.2.2 remains open. Work plan keeps task-source checkboxes visible as
source claims and shows a coordinator outcome only while the selected campaign's
status is a completed live observation. A status refresh in progress, a failed
refresh or an aged observation hides retained outcome badges and selected-task
source detail. The screen says why the coordinator outcome is unknown and offers
an explicit read-only **Refresh coordinator outcomes** action. Its adjacent
**Show command** disclosure presents the exact `campaign-status` argument vector
for the selected configuration and campaign. The action is disabled when the
binary or path cannot be represented safely. An absent campaign status is shown
as unstarted rather than as a verified zero.

Unknown badges include the task identifier: in a software-rendered diagnostic,
repeated generic "Outcome: unknown" text was visible but absent from the
accessibility tree. The focused test now requires the named task badge there.

An empty Work plan links to Setup and offline Help. A malformed task source
shows the parser failure and the same recovery destinations; navigation never
edits the task source or authorizes a campaign. A refreshed live status restores
the outcome display only after the coordinator response is accepted. This is a
bounded Work plan slice; other screens, in-context depth, action disclosures,
desktop accessibility, human trials, live-provider qualification and release
gates remain open.

## Verification disposition

The first focused campaign/workplan batch and a focused retry failed a new
assertion: repeated generic unknown badge text was visible in the
software-rendered frame but absent from the accessibility tree. A diagnostic
test-only edit also produced a compile failure. A later retry checked for
restored outcomes before the asynchronous head-plan response; the corrected
test waits for both observations. These failures are retained in the private
session record. The corrected focused real-coordinator workflow passed
1/1 with task-specific accessible unknown and restored completed labels.
The software-rendered native UI all-target suite passed 147/147 tests across
14 targets, including all seven Work plan tests and nine campaign tests. That
run preceded two style-only corrections: removal of an unnecessary semicolon
and extraction of the new test steps into a helper. The final-source focused
library, campaign and Work plan batch then passed 87/87, and final-source
strict workspace Clippy with warnings denied passed. The initial Clippy
failures and their corrections are retained in the private session record.

Python unittest ran 42 tests: 41 passed and the sole failure was the unchanged
CM-R01.6 eight-input source-bound evidence-freshness binding; no digest was
renewed. Formatting, documentation, architecture, no-write inventory and
whitespace checks pass. The reviewed inventory generator reports 1,767 items
and 825 explicit gaps: one line-derived public item ID moved, with no semantic
item, category, mapping or gap change. The exact staged diff and additions
were inspected for scope and private host or credential material. This
increment's exact-SHA independent review passed after separate Work plan,
Campaign and Reports focused tests and strict workspace Clippy. The review
does not close Task 36.3.2.2 or the human/live gates.

The two earlier independent reviews of the preceding source-inspection
correction remain inconclusive solely because neither could acquire the shared
heavy-build reservation for independent Rust tests. Their source review found
no new code issue; neither is an independent PASS.
