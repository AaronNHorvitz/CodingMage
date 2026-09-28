# Sprint 36 Campaign Changes Projection

Task 36.3.2.2 remains open. This increment routes the Changes screen's commit and changed-path
summary through `campaign-changes` under exact campaign, repository and reconciled-head
authority. It removes the interface's direct Git subprocess path. The projection is bounded,
versioned, rejects malformed output and exposes truncation. The UI checks all identity and
range fields against its current observations before rendering. A repository object summary
does not establish review, accepted completion or delivery.

The private run-record scan, full state catalogue, contextual depth and remaining exact Show
command affordances remain open. Desktop, Orca, live-provider, human trial, independent review
and release gates remain unperformed. CM-R01.6's source-bound package-renewal failure remains
open; these new source files do not renew that evidence.

The first focused compile failed because a byte slice has no `split_terminator` method. That
failed attempt is retained privately. The parser was corrected to require a trailing NUL and
split the bounded byte slice. The corrected Git tests passed 2/2 and the sibling CLI built. The
first Changes integration run passed 4/5; a forged projection exposed that the UI observation
retained its old successful value when parsing failed. The UI now clears that value before
recording the error. The focused failure is retained and the corrected focused rerun passed 1/1.
On the final source, the sibling CLI build, strict workspace Clippy, Git library tests (40/40),
CLI library tests (4/4), and UI all-target tests (82/82) passed. Python unittest ran 42 tests:
41 passed and the one retained CM-R01.6 source-bound freshness test failed on the same eight
input-drift paths. No digest was refreshed and this is not a green Python gate. A second Python
run after the final TASKS prose correction was queued but never acquired the shared reservation;
it was cancelled without a test result. The production code was unchanged by that correction.
Formatting, documentation, architecture, inventory validation and diff checks passed. The
private logs retain both failed iterations, the focused correction and the final cumulative
output. The deterministic inventory was regenerated after reviewing the generator's declaration,
test and schema inputs: 1,696 public/error surfaces and 825 explicit keyword-mapping gaps. This
inventory is candidate mapping evidence, not proof that each declaration was tested.
