# Public Setup export notice clearing

Task 36.3.2.2 remains open. Decision 0092 moves the manual Setup export
notice-clear action and the post-verification cleanup to
`codingmage setup-export-clear`. The native UI displays and submits the same
argument vector. `--verified true` rechecks the prior successful export and
current destination bytes. `--verified false` is the explicit manual-inspection
path and does not assert export success. Both paths require the exact private
intent digest, request, configuration and repository identity and refuse a
running detached helper. Only the private intent and matching terminal notice
are removed, in recoverable order; the destination stays untouched.

The command uses a held directory descriptor, no-follow held file reads,
owner/non-writable-ancestor and private-leaf mode checks, the helper's
nonblocking lock and named-leaf identity checks. It returns a closed,
content-minimized clear receipt. The UI rejects an unknown field, wrong
request, wrong verification type, stale
selection or failed command; it keeps the notice until it sees a matching
receipt. The real-process Setup target exercises the manual Show-command,
failed-export inspection and clear, automatic verified cleanup, reopen,
malformed terminal and helper survival across window close.

The first full Setup run against this change failed 15/20 because the public
clear command rejected a test-injected state root name, including the custom
private directory used by the native fixture. The directory check now binds
the `projects` hierarchy to the configuration hash and verifies ownership and
private permissions without assuming a single state-root name. An initially
focused CLI compile failed on an `OsStr`/String comparison, and a later
test-only compile failed on a shadowed path variable; both were corrected.
These failures remain recorded here and in the private handoff. A subsequent
focused CLI test passed 1/1 and the native Setup target passed 20/20. The
CLI all-target suite then passed 82/82 with two sustained-soak tests ignored
by their explicit qualification gate. It ran while the last private-lock and
directory checks were tightened, so that broad pass is recorded as
pre-final-source evidence. The final-source focused clear test passed 1/1
and strict workspace Clippy with warnings denied passed. Final native UI,
Python and static dispositions follow. No installed,
assistive, human, live-provider or release claim follows from these local
tests.

The inventory source and schema were inspected before regeneration. The
logical inventory remains at 1,885 surfaces and 826 explicit gaps: the old
private `clear` and `clear_notice` surfaces are replaced by the CLI `clear`
and native `clear_arguments` surfaces. No existing common surface changes
applicability and the normalized gap set remains the same. Heuristic test
mappings are prompts, not coverage certificates. A final no-write check found
only 40 line/ID shifts after the Setup helper was refactored; the source and
schema were rechecked, the inventory regenerated, and the no-write check
passed again with no logical item, applicability or gap change.

The first final-source native all-target invocation passed library 107/107,
Campaign 15/15, Changes 12/12, Content 1/1, contract parity 4/4,
Execution 7/7, Help 2/2, Readiness 6/6 and Reports 7/7, then stopped at
Setup 19/20. The failure was a test query that matched both newly visible
clear previews by their common command prefix. The fixture now selects the
distinct `--verified true` and `--verified false` previews. Its focused
rerun passed 1/1; Setup then passed 20/20, Shell 18/18, Verification 8/8
and Work plan 7/7 on unchanged production code. This is cumulative native
coverage of 214/214 across 13 nonempty targets, with labelled offscreen
software rendering and no GPU. It is not one uninterrupted all-target pass;
the original failed assertion is retained.

Final strict workspace Clippy with warnings denied, `cargo fmt --check`,
the no-write verification inventory, `docs_check.py`, architecture checks
and diff whitespace passed. The full Python suite ran 50 tests: 49 passed,
and only the unchanged CM-R01.6 evidence-binding test failed with its eight
previously recorded source-bound input drifts. The original failed receipt,
package and binding were not renewed. Independent review of exact commit
`8a46f406b9204347682d299adbd1f3fb033b9891` found a High
verified-clear final-verification race. Decision 0093 and
`sprint-36-setup-export-clear-race-correction.md` record the correction;
its exact-commit re-review remains open. The full section-five state/depth/Show-command
catalogue and guided configuration recovery remain open, along with all
installed, assistive, human, live-provider and release gates.
