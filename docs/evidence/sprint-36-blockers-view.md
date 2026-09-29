# Sprint 36 dedicated Blockers view

Task 36.3.2.2 remains open. The Blockers destination presents the selected campaign's bound
`campaign-explain-blocker` observation through the existing asynchronous command client. It
keeps the coordinator's `null` response distinct from unrequested, loading, stale and failed
observations. A non-null payload still must identify the selected campaign, and a later failed
refresh retains the earlier value with an explicit stale warning. The view separates the
campaign-level code, blocked tasks, deferrals with trigger state, and human decisions. Search
filters the coordinator-reported codes and task identities; a 100-row bound per category avoids
rendering an unbounded list and tells the operator to narrow the query. The coordinator phase and
pending, satisfied or unfamiliar trigger states retain separate wording.

Refresh is read-only. Its enabled control has an exact **Show command** preview. Selecting a
repository or campaign alone does not clear, admit or start anything. The view links to Work
plan, Campaign and offline Help for inspection. Clearance of external prerequisites and
deferral triggers remains a coordinator command with an operator-supplied request identity and
evidence digest. The UI does not synthesize these values or imply that a hold was cleared.

Static view wording is in the versioned English catalogue and participates in the synthetic
40% expansion and right-to-left text variant test. This is layout stress evidence, not a real
screen-reader or installed desktop result. Focused real-process empty-state, bound-payload,
keyboard-navigation and no-match search checks pass. The final software-GL native UI all-target
suite passes 106 tests, and strict workspace Clippy passes. Formatting, documentation,
architecture, regenerated inventory and diff whitespace checks pass; the inventory contains
1,729 public items and 825 explicit gaps. The Python suite passes 41 of 42 tests. Its only
failure is the unchanged eight-input CM-R01.6 source-bound evidence drift, which needs a
separate qualified-human candidate-construction review before renewal.

The first focused build found two `String` borrow mismatches in error rendering. The corrected
build reached the view test, whose first draft used an absence query that panics when it finds
no node; a later keyboard-driven no-match query verifies the actual empty-search screen. The
failed receipts are retained in private state. Full per-code clearance actions, complete
per-screen state/depth coverage, independent review and human trials remain open.
