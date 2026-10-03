# Public Setup export recovery inspection

Task 36.3.2.2 remains open. Decision 0091 moves the read-only "Check previous
export outcome" action from a private UI-worker file read to the public
`setup-export-recover` CLI command. The native worker passes the exact
argument vector shown in the Setup disclosure, with a bounded deadline. The
command holds the named intent, terminal and destination files without
following their leaf symlinks, checks the submitted intent digest, request,
configuration and repository identity, and returns a closed content-minimized
verification receipt. No document bytes or credential appear in stdout.

The interface accepts only a matching version-one response for its current
intent and selected repository, then separately asks the worker to clear the
private notice with the existing destination and terminal recheck. A command
failure, missing terminal record, changed destination, stale selection,
malformed response or queue refusal leaves the notice unresolved. The new
command is read-only and does not cancel or restart a detached export.

The synthetic CLI unit exercises success, foreign repository, stale digest,
changed destination, malformed terminal JSON and a linked result. The native
receipt test checks the exact preview, matching response, foreign request,
unknown field and malformed JSON. The first focused CLI test passed 1/1 and
the first native response test passed 1/1. Strict target Clippy initially
found a digest-format allocation lint; it was corrected and the target lint
passed. CLI all-target tests then passed 81/81 with two pre-existing
sustained-soak tests ignored by their explicit authorization gate. The
real-process native Setup case was extended to display the new command,
invoke it against a malformed retained terminal result and prove the notice
stays unresolved; its focused execution passed. The first native all-target
run passed the 108-test library target, Campaign 15/15, Changes 12/12,
Content 1/1, contract parity 4/4, Execution 7/7, Help 2/2, Readiness 6/6 and
Reports 7/7, then stopped at Setup 19/20. The failing test still waited for
the old UI-local `setup_export_invalid_receipt` code after a named destination
was replaced. The new public command returns `codingmage.cli.stale_observation`
for that condition. The fixture now requires that exact refusal and the
retained intent/terminal notice; its focused rerun passed. The initial timeout
remains a failed test result, not a pass.

This increment does not supply public command equivalents for the manual
export notice-clear or guided configuration recovery controls. The full
section-five state catalogue and in-context depth are still open. Local
synthetic checks are not installed desktop, assistive-technology, human-trial,
real-provider or release acceptance. CM-R01.6's source-bound evidence package
remains unchanged.

The inventory generator's source and applicability rules were inspected
before regeneration. Version-one inventory now has 1,885 surfaces and 826
explicit gaps, from 1,884 and 826. The new public CLI recovery command and
native argument builder replace the old UI-local recovery function. No existing
surface changed applicability and the normalized gap set is unchanged.
New command input parsing has all six applicable categories; the argument
builder has positive, negative, boundary and repeatability categories. The
inventory's heuristic test mappings are prompts for verification, not a
coverage certificate.

The final Setup target passed 20/20, Shell 18/18, Verification 8/8 and Work
plan 7/7. Together with the unchanged production source's earlier passing
library and integration targets, the native suite covered 215/215 tests
across 14 targets under one Cargo job and one test thread. It did not finish
as one uninterrupted all-target invocation because the first run stopped at
the old assertion. Verification used labelled offscreen software rendering
with no GPU. Strict workspace Clippy with warnings denied passed.

The first full Python run had 48/50 passing: CM-R01.6 and an owned inventory
test still naming the removed UI-local `recover` function. The test now
tracks the public CLI inspector and native argument builder, with the old
surface absent. The final Python run had 49/50 passing; only the unchanged
CM-R01.6 freshness test failed with the same eight input drifts. Its
original failed receipts, package and source binding were not renewed.
Independent exact-commit review remains pending.
