# Sprint 36 Reports source recovery increment

Task 36.3.2.2 remains open. Reports now sends an empty screen directly to Setup when no
repository is open and to Campaign when a repository has no selected campaign. Help is
available in either state. A selected campaign shows separate status and blocker-explanation
observations, including age, loading, failure, retained stale values, never-requested values
and the coordinator's observed empty responses. It offers read-only refreshes through the
existing `campaign-status` and `campaign-explain-blocker` argument builders. Each refresh has
an exact Show command disclosure; absence of a safely representable command disables the
control. The source panel points to Changes and reviews for change/run evidence and Campaign
for the final coordinator report. No report view, navigation action or refresh starts an agent,
changes a task state or grants authority.

The source-state wording is bundled in the existing versioned English message catalogue.
The existing pseudo-locale validator expands those keys by at least 40%; the display-less
Reports empty preview checks labels at 1024×640 and 200% scale with a right-aligned synthetic
layout. This is a local label stress test, not installed assistive-tool verification or full
bidirectional navigation. Coordinator failure codes and dynamic report content remain outside
the static catalogue.

The disposable-repository integration test checks no-project and no-campaign navigation,
shows both exact command previews, injects malformed successful output after two real
observations, confirms failure and stale guidance without treating retained empty values as
current, then requests both reads through the UI and observes recovery. A tree digest proves
the target repository did not change. The first focused run exposed a test query matching
multiple error labels; the corrected query then exposed missing stale guidance for retained
empty observations. Both failures were retained privately. The corrected focused recovery
case passed 1/1.

The inventory generator and version-one schema were reviewed before regeneration. The
generated index has 1,753 public items and the same 825 explicit gaps: two status-method
surfaces became visible to the inventory, 14 existing line-derived IDs moved, and 193
unchanged-ID entries have changed crate-wide capped heuristic test mappings. No schema,
stable coordinator error code, applicable gap or claimed coverage changed. The mappings
remain an index, not proof that a listed test covers each public item.

## Verification disposition

The corrected Reports target passed 5/5. The first cumulative run reached Reports after
passing its earlier targets, then failed only because a new test queried lowercase `select`
for a capitalized catalogue sentence. The corrected source passed 134/134 native UI tests
across 14 targets. Strict workspace Clippy first found that the expanded Reports renderer
exceeded its 100-line limit; extracting its existing export controls preserved the behavior.
On that final structure, strict workspace Clippy passed and a fresh software-GL native UI
all-target run passed 134/134. Formatting, documentation, architecture, no-write inventory
and diff-whitespace checks passed. Python unittest ran 42 tests: 41 passed and the sole
failure was the unchanged CM-R01.6 eight-input source-bound evidence drift. No digest was
renewed. The final native UI receipt is retained privately for independent review.

Report assembly and inline JSON preview still run during rendering. A public matching
`codingmage` report-export command, full section-5 coverage on every screen, full dynamic
message externalization, installed desktop accessibility and human trials remain open. No
live-provider or release qualification follows from this local fixture.
