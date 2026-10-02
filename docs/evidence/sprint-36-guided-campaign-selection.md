# Guided campaign selection through the public coordinator read

Decision 0083 addresses one remaining Task 36.3.2.2 boundary. The
`campaign-select` command now returns a closed version-two snapshot with the
validated authority, its selected path, canonical authority digest, and the
length and SHA-256 of the exact TOML bytes read from the same held no-follow
file. The native decoder rejects version one, malformed fields, an unexpected
path or repository, and an invalid authority digest.

The guided campaign worker runs the displayed authorization inspection,
private-input write and public selection commands in order. It checks the
write receipt against the submitted candidate, then checks the selection
snapshot's exact source bytes against that receipt and its campaign, head,
task-source and repository identities. The native window adopts only the
matching response for its current request, selection generation and binding.
The Setup form exposes “Show command” for all three steps. The previous
interface-thread destination read and parser were removed. This does not
admit or run a campaign; a later preflight still governs execution.

Disposable tests exercise a real public command and source-byte digest,
changed length/digest refusals, malformed and unsupported snapshots, file
replacement after a write receipt, a normal guided write and a bad
authorization inspection that never writes. The first exploratory focused
run used an older sibling coordinator executable and was stopped after a
failure; it is not accepted evidence. After rebuilding the coordinator, the
campaign target passed 15/15. That run's Setup target passed 15/16; its sole
failure was an assertion looking for the former error wording. The corrected
Setup target then passed 16/16. Both failures remain in private test logs;
neither was relabelled as a product pass.

Strict workspace Clippy with warnings denied passed after refactoring an
overlong fixture and the worker function signature. The final native UI
all-target suite passed 204/204 across 14 targets under the shared build
reservation, one Cargo job, one test thread and labelled software rendering.
The shared campaign library passed 48/48. This increment adds no dependency,
runtime model qualification, GPU use or external publication authority.

The verification inventory source and applicability rules were inspected
before regeneration. It now has 1,843 items and 826 explicit gaps, compared
with 1,842 items and 826 gaps at the parent: three new public surfaces and
two removed direct-read surfaces. The existing campaign loader's previously
omitted malformed/unknown-input applicability was explicitly corrected
because the shared parser rejects those inputs; no other existing
applicability or gap tuple changed. The Python guard initially failed because
it still named the removed direct reader. The corrected guard checks the new
receipt-bound decoder and actual public-symbol presence. Inventory test-name
suggestions are not accepted coverage evidence.

## Final verification disposition

Formatting, documentation, architecture, inventory no-write and whitespace
checks passed on the final source. The full Python suite ran 49 cases after
the guard correction: 48 passed and only the unchanged CM-R01.6 eight-input
source-bound `input-drift` check failed. Its original failed receipts and
binding were retained; no digest, package or review approval was renewed.
The first Python run's additional stale-name guard failure is retained in
private test output and is not counted as a passing run.

Task 36.3.2.2 remains unchecked, and this increment awaits fresh bounded
independent review. CM-R01.6 qualified-human source-bound review, installed
desktop and screen-reader checks, human trials, separately admitted
real-provider runs, full-product review, licence and release decisions
remain open.
