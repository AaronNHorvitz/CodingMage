# Setup export verified-clear race correction

Task 36.3.2.2 remains open. Independent bounded review of exact commit
`8a46f406b9204347682d299adbd1f3fb033b9891` returned `FINDINGS`: a High
race let verified clear validate a newly opened terminal or a destination
changed after verification, then remove the prior notice. The review did not
qualify human, live-provider, installed-desktop or whole-product acceptance.

Decision 0093 changes the mutation path to validate the exact held terminal
bytes and recheck intent, terminal, destination and lock identities at the
removal boundary. The read-only recovery path remains separate. The focused
regression replaces the terminal, destination and named lock with same-content
files after the initial verification; each replacement must return stale and
retain both private records and the destination.

The first focused CLI race test passed 1/1. A follow-up also asserts that a
terminal appearing after a manual clear observed its absence is refused with
both records retained; the focused rerun passed 1/1. The first strict
workspace Clippy run found function-length and nested-check warnings after
this follow-up. A small destination-check helper resolved them and the final
strict Clippy run passed with warnings denied. Formatting, documentation,
architecture and whitespace checks passed. The reviewed inventory source and
fixed applicability produce 1,885 surfaces and 826 explicit gaps; the only
changes are two line-derived IDs for the moved public functions, with no
applicability or gap change. Its 11 focused Python tests passed.

The production-source CLI all-target invocation passed library 23/23, mission
16/16, preflight 1/1, campaign repair 2/2, directory 3/3, prescribed
2/2, project-open 4/4, repair 4/4, authorization 2/2, campaign Setup
9/9 and configuration Setup 3/3. Workflow passed 12 cases, failed one
five-pod parallel campaign case and ignored two sustained qualification
tests under their explicit gate. The failure was `Blocked` where the fixture
expected `Complete`; a test-only assertion now prints the bounded campaign
outcome on future failure. Its exact focused rerun passed 1/1 in 89.85 s.
The all-target invocation remains failed; the focused pass does not erase it
or establish sustained five-pod qualification. The changed production path
is Setup export notice clearing, separate from this campaign workflow.

The native Setup real-process target passed 20/20, including public-command
export verification, replaced destination refusal and detached-helper
recovery. These are display-less fake-provider tests with labelled software
rendering, no GPU or installed-desktop claim. The full Python suite passed
49/50: only the unchanged CM-R01.6 evidence-freshness binding failed with
the same eight input drifts. Its original failed receipt, package and
binding were not renewed. The exact staged nine-file diff, including the
two line-ID-only inventory changes, was inspected. Added lines were scanned
with no private host path or credential markers; staged whitespace
passed and no unstaged changes remained. Exact-SHA independent re-review
follows after the coherent commit. This correction alone does not
close the full section-five state, contextual depth or Show-command
catalogue.
