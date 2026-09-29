# Sprint 36 blocker action guidance

Task 36.3.2.2 remains open. The native Blockers view now gives contextual guidance for the
coordinator's seven closed blocked-task reasons, its human-decision reasons and the six
deferral triggers. The key distinction is whether a pending trigger can be observed through
`campaign-observe-trigger`. Only `provider_reset`, `review_completion` and `operator_resume`
accept that evidence-bound command. Head advancement, lease release and gate-resource release
are coordinator-observed; the view no longer directs an operator to submit that command for
them. Unrecognized codes, mismatched reason/trigger pairs and unknown trigger states receive
neutral inspection guidance.

The view still changes no campaign state. A coordinator command, when eligible, needs an
operator-supplied request identity and evidence digest; the interface creates neither. Its
guidance does not declare a prerequisite resolved, a human decision made or a campaign resumed.
The exact supported codes were reconciled against `codingmage-contracts::campaign` and the
runtime's `externally_observable_trigger` boundary before this mapping was written.

The focused real-process Blockers view test passed 1/1. A cumulative software-rendered
native UI all-target run passed 109/109, then strict workspace Clippy found an overlong test
function. Its fixture and search steps were extracted into helpers. The subsequent exact-tree
UI library run passed 47/47, the focused real-process view passed 1/1 again and strict
workspace Clippy passed. The final change after the cumulative UI run also rejects unknown
and mismatched reason/trigger pairs; its cross-crate closed-code and negative tests ran in
the exact-tree library batch. Formatting, documentation, architecture, regenerated inventory
and diff whitespace checks passed. The inventory remains at 1,729 public items and 825
explicit gaps, with no added or removed public surface; heuristic test references changed.
The first Clippy failure is retained in private test output, not represented as a pass.
The Python unittest suite passed 41 of 42 cases. Its sole failure is the unchanged CM-R01.6
source-bound evidence drift on eight earlier inputs; no digest or human approval was changed.

These tests use synthetic coordinator observations and a real disposable campaign for the
bound view; they are not live provider, installed desktop, screen-reader or human-trial
qualification. The independent review and the separate CM-R01.6 qualified-human
source-bound evidence gate remain open.
