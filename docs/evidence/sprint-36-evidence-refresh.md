# Sprint 36 native evidence refresh increment

Task 36.3.2.2 remains open. The Changes and evidence destination now offers separate read-only
refreshes for exact-head candidate changes and checkpoint-bound run evidence. Each control has a
Show command section built from the same argument vector it submits. A stale, missing or failed
campaign status disables manual evidence reads until the owner refreshes status on Campaign.
The existing coordinator response binding still rejects a foreign repository, campaign, head
or status checkpoint.

The destination names loading, unobserved, stale and failed projections separately. Known
backend failures use the shared cause classification and state the effect on review and
completion claims. The English labels and effects are in the bundled message catalogue. The
empty destination leads to Setup when no repository is open and to Campaign when no campaign is
selected. The screen continues to state that engineering completion is separate from delivery.
It cannot invent findings or full logs that the bounded coordinator projection does not retain.
No new command, dependency, provider, credential, authority or publication effect was added.

The focused display-less test uses a disposable repository, the real coordinator executable
and a fake provider campaign. It opens both exact command previews, activates the changes
refresh, injects malformed change and run responses with the current request binding, checks
that neither failure becomes evidence, refreshes each read successfully, then injects a failed
status observation and confirms direct refresh methods stay inactive. This is local synthetic
fault injection, not live-provider or installed-desktop qualification.

## Verification disposition

The first focused fixture passed 1/1. The extended fixture, including both change and run
recovery, passed 1/1. The software-rendered native UI suite then passed 114/114 tests across
all targets. Inspection found that a direct refresh call should evaluate status age against the
current clock even when no frame had recently rendered. An empty-state navigation correction
then gave the destination a direct next step for both missing selections. On the final source,
the focused Changes target passed 3/3, including both corrections, and strict workspace Clippy
with warnings denied passed. The full UI suite preceded these narrow corrections and is not
represented as an exact-final-tree run.

Formatting, documentation, architecture, generated-inventory validation and diff whitespace
checks passed on the corrected source. The reviewed inventory has 1,735 public items and 825
explicit gaps: two new UI refresh methods, no removed surface, 333 stable error codes unchanged,
13 shifted line-derived identifiers and no changed heuristic test mappings or gap set. Python
unittest ran 42 tests: 41 passed, and only the retained CM-R01.6 eight-input source-bound drift
failed. No evidence digest or candidate-construction binding was renewed.

Independent exact-commit review remains pending. CM-R01.6 qualified-human candidate
construction, real desktop and assistive-tool verification, human trials, live-provider
qualification and release remain open.
