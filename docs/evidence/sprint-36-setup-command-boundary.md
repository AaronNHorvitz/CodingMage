# Public Setup command boundary evidence

## First public write: authorization record

Task 36.3.2.2 remains open. Decision 0064 introduces the public
`setup-write-authorization` command as the first bounded Setup write. Its
arguments identify an existing configuration, the exact repository ID observed
by `doctor`, and a requested output outside that repository. A stale or
cross-project identity is refused before stdin is read. The held repository
identity and Git HEAD are revalidated at the final publication boundary. The exact record is
sent by stdin, limited to 1 MiB, validated as nonblank UTF-8 without NUL, and
never put in arguments, stdout, stderr or a preview. The result is a
content-minimized JSON receipt
with the bound repository identity and SHA-256 digest. The reviewed CLI report
writer supplies no-overwrite by default, explicit atomic replacement, and
the previously recorded uncertainty behavior.

The final-source real-process disposable test covers an initial private-mode
file, digest receipt, no-overwrite refusal, explicit replacement,
blank/malformed/NUL/oversized input, stale repository identity, output inside
the repository and invalid overwrite grammar. Its code and test names are in
`crates/codingmage-cli/tests/setup_authorization.rs`.
Both real-process tests pass (2/2). CLI unit tests pass (20/20), including
side-effect-free exact help and the reviewed report writer cases. Existing
core repository revalidation tests pass (2/2) for a changed Git HEAD and a
replaced target directory. Strict workspace Clippy with warnings denied,
`cargo fmt --all -- --check`, documentation checks, architecture checks,
no-write verification inventory and `git diff --check` pass. The Python suite
passes 42/43 tests with the sole unchanged CM-R01.6 eight-input source-bound
freshness failure; the package and failed receipt were not renewed. A broad
CLI all-target run completed the unit (20/20), mission (16/16), preflight
(1/1) and repair (2/2) targets before it was stopped during a longer campaign
target because the post-input authority revalidation source edit made that
run non-final. It is a partial regression check, not a full-suite pass. The
first focused compile failed because the installed Rust toolchain does not
support the attempted `Result::is_err_or`, and the pinned SHA-2 type does not implement
`LowerHex`. Both were corrected before rerunning; the failed output remains
in private session evidence. This is not a behavioral failure or a test pass.

Configuration, campaign and document-export writes still run in the UI
process, and this authorization action has not yet been routed through a
bounded native worker. Exact Show-command and stdin disclosure remain open.
Fake or disposable tests do not establish human, installed-desktop or live
qualification. No old review report or source-bound package digest is
changed. The generated inventory has 1,774 items and 825 explicit gaps;
the gap set is unchanged. One public Rust surface was added, and line IDs
and heuristic test mappings shifted. Those mappings are not verified coverage.

## Independent finding and local correction

Independent review of `d77a67414af09b5669c1483c21e442d5d8fe0454` returned
`FINDINGS` with one High item. A disposable real-process test delayed candidate
sync, changed Git HEAD before publication and observed a success receipt. The
report is retained privately and has not been edited. The initial results
above remain historical results for that source, not a pass for the correction.

Decision 0065 adds a caller-supplied authority check after candidate byte and
destination validation, directly before create-only linking or overwrite
exchange. A second check after publication suppresses a success receipt if the
repository changes during publication; because bytes may already be visible,
that path returns `codingmage.cli.uncertain_write` and retains the private
stage for reconciliation. A deterministic test changes fixture Git HEAD at
each boundary for both create and overwrite. Before publication it requires
`stale_observation` with no new or replaced public leaf; after publication it
requires `uncertain_write`, the published leaf and retained reconciliation
stage.

On the corrected source, CLI unit tests pass 21/21, including the four timing
and overwrite cases in the new deterministic test; the real-process
authorization tests pass 2/2. Strict workspace Clippy with warnings denied,
Cargo formatting, documentation and architecture checks, no-write inventory
and diff whitespace pass. The Python suite passes 42/43 tests with only the
same CM-R01.6 eight-input source-bound freshness failure; no digest or
package was renewed. Initial strict Clippy runs found an owned pass-by-value
lint and then test fixture naming/length lints. All three were corrected;
their failed outputs remain in private session evidence. No corrected-source
behavioral test failed.

The inventory was source/schema-reviewed and regenerated: 1,774 to 1,775
items, 825 explicit gaps before and after, with the normalized gap set
unchanged. The new public Rust surface is the guarded writer; three other
line-derived IDs moved and 18 existing entries changed only in location or
heuristic test mappings. These mappings are not coverage proof. This local
correction was independently re-reviewed at exact commit
`46aeae8da288c46ba3e76a38c59eb4bb745b129b` with a bounded PASS and no
finding. The reviewer reproduced the delayed-staging HEAD change and both
post-publication uncertainty cases. This does not qualify the native worker or
the human, live and release gates.

## Native private-stdin worker increment

Decision 0066 routes the authorization action through the bounded native
worker. The worker transports at most 1 MiB of private stdin to the public
`codingmage setup-write-authorization` command off the render thread. The
record text is absent from arguments, command preview and debug output. The
screen binds the request to its selection generation, exact diagnosis
repository ID and request identity; only a schema-1 receipt matching the
requested repository ID, byte count and SHA-256 digest selects the record.
Queue refusal, command error, cancellation and malformed or mismatched receipt
leave success unclaimed and ask the owner to inspect the destination before
retrying. Switching authority invalidates a pending request and its response.
An older response cannot clear a newer pending write with a different request
identity. There is no automatic retry. The in-context Show-command displays
the exact argument vector and discloses that the private stdin bytes are omitted.

This increment does not route configuration, campaign or export writes through
public commands, and it does not complete the state/depth catalogue in Task
36.3.2.2. The native UI all-target suite passes 175/175 across 14 targets.
The UI library target passes 95/95, including private
transport, finite deadline and malformed/foreign receipt cases. The
real-process Setup target passes 4/4, including repository-contained refusal,
duplicate pending action, successful external record and discarded
cross-repository response. The native verification target passes 7/7,
including a disposable setup-to-outcome workflow, recovery, keyboard traversal
and window sizes. Strict workspace Clippy with warnings denied passes. The
full Python suite passes 43/44 with only the unchanged CM-R01.6 eight-input
source-bound freshness failure; no binding or package digest was renewed.
Cargo formatting, documentation, architecture, no-write inventory and
whitespace checks pass. The first Clippy attempt found three owned style
lints, and a subsequent focused Setup attempt found a duplicate contract-code
explanation. Both failed attempts were corrected before the final-source
checks and remain in private session output. These local results are not an
independent review. Separate human, installed-desktop and live-provider
qualification remains open.

## Native pending-command review correction

The independent bounded review of the native worker commit
`99017d8b1fb6408659c6be2210da6656229ef2da` returned PASS with one Medium
finding. A pending write kept the submitted CLI arguments in the worker, but
Show command recomputed them from editable form fields. Editing the path or
overwrite choice while pending could show a different command from the one
already queued. The private review report is retained; its PASS is
scoped to that exact source and does not approve the correction.

The correction stores a copy of the submitted public argument vector in the
pending request, displays it until that request resolves or is cancelled, and
disables path, text and overwrite edits while pending. The display-less
regression deliberately mutates all three fields despite the disabled widgets,
checks the displayed command against the queued vector and confirms that only
the originally requested destination receives the original bytes. The focused
regression passes 1/1, the complete Setup target passes 5/5, and the native UI
all-target suite passes 176/176 across 14 targets. The first strict Clippy
attempt found an owned test fixture missing the new pending argument field;
it was corrected before the all-target run. The regenerated verification
inventory remains at 1,781 items and 825 explicit gaps, with no changed
normalized surface, applicability, test mapping or gap. Strict workspace
Clippy with warnings denied, Cargo formatting, documentation, architecture,
no-write inventory and diff whitespace pass. The Python suite passes 43/44;
its sole failure is the unchanged CM-R01.6 eight-input source-bound freshness
hold. No package, input digest or binding was renewed. Exact-commit
independent re-review of `0362c3f012e4319dc19050dc9de89909a50657c3`
returned PASS without findings. Task 36.3.2.2 remains open.

## Public campaign Setup command increment

Decision 0067 adds `setup-write-campaign` to the coordinator CLI. It accepts
candidate TOML only on bounded private stdin and validates it with the same
strict parser as `CampaignSpec::load`. The public arguments bind the exact
configuration, repository ID, clean Git HEAD, committed task-source digest,
external authorization record, destination and overwrite choice. The command
compares all authority-bearing fields to fresh observations and checks the
configuration, record and repository/source again at the guarded publication
boundary. It refuses to replace the configuration or authorization record
as the campaign destination and refuses to treat the configuration as that
record. Its receipt contains only repository/campaign
identity, HEAD, byte count and candidate digest; it does not admit or start a
campaign.

The campaign parser focused test passes 1/1 for exact-byte parity, malformed,
unknown-field and oversized input. The disposable real-process CLI target
passes 3/3: exact-byte publication and replacement, stale/cross-project
bindings, malformed and oversized input, protected destinations, dirty task
source and changed committed head. The first focused CLI test compilation
failed because the pinned SHA-2 type does not implement `LowerHex`; the test
hex helper was corrected and the failed compile remains in private tool output.
This CLI-only increment leaves the native campaign form's direct write and
its Show-command preview open. Independent review has not evaluated this
increment. Human, installed-desktop, live-provider and release gates stay
open; the CM-R01.6 freshness binding is not renewed.

On the final source, parser 1/1, public CLI help 1/1 and the real-process
campaign Setup target 3/3 pass. Strict workspace Clippy with warnings denied,
Cargo formatting, documentation, architecture, no-write inventory and staged
whitespace checks pass. A broader CLI/campaign all-target run passed 111 tests
with zero failures and two explicitly ignored sustained qualification tests;
small parser and test changes made after it launched mean this is preliminary
regression evidence, not exact-final-source all-target evidence. The Python
suite ran 44 tests: 43 passed and only the unchanged CM-R01.6 eight-input
source-bound freshness test failed. No package, receipt or digest was renewed.
The source/schema-reviewed inventory is 1,783 public items and 825 explicit
gaps. Two normalized public surfaces were added, none removed; no existing
applicability or normalized gap changed, and nine existing heuristic test
mappings shifted. A mapping is not proof of coverage. Independent exact-commit
review of this new CLI command returned `FINDINGS` at exact commit
`4c4afe0fbe98dd96c2a45c6bab8cde10d5b4c283`; see the correction below.

## Campaign Setup physical-file correction

Independent exact-commit review of `4c4afe0fbe98dd96c2a45c6bab8cde10d5b4c283`
returned `FINDINGS` with one High item: a configuration hard link under a
different name passed as the external authorization record when the candidate
contained the matching digest. Its disposable reproduction and review report
are retained privately, unedited. The earlier CLI tests and checks above
remain historical evidence for that source, not a pass for the correction.

Decision 0068 holds no-follow descriptors for the configuration and external
record, compares their device/inode identities, reads the bounded record from
the held descriptor, and rechecks each named leaf and the protected output
inodes at the guarded publication boundary. Real-process regressions cover
the matching-digest linked-record exploit, hard-linked output aliases for
each protected file, and parent-directory aliases. Initial focused campaign
Setup results: 6/6 pass on corrected local source. The exact-source broader
CLI/campaign all-target run passed 112 tests with no failures and two
explicitly ignored sustained qualification tests. Strict workspace Clippy
with warnings denied, Cargo formatting, documentation, architecture,
regenerated/no-write verification inventory and diff whitespace pass. The
source/schema-reviewed inventory remains at 1,783 items and 825 explicit
gaps, with no normalized surface, applicability, heuristic test mapping or
gap change; only line-derived locations moved. The full Python suite passed
43/44 tests; the sole failure is the unchanged CM-R01.6 eight-input
source-bound freshness hold. No package, receipt or digest was renewed.
The initial correction results above preceded independent re-review. Human,
installed desktop, live-provider and release gates remain open.

The bounded read-only exact-commit re-review of
`1ce1f8f0f4393c1f552e080d939beea7b9b609f2` returned PASS with no open
finding. It independently reproduced the original matching-digest hard-link
exploit as a refusal, checked linked protected output and parent aliases, and
used a reviewer-only held-stdin fixture to replace the authorization leaf
after initial observation. The command returned stale observation and wrote no
output. The reviewer also passed the focused Setup target 6/6, strict Clippy,
documentation, architecture and inventory checks; Python retained only the
unchanged CM-R01.6 eight-input freshness failure. This bounded PASS does not
qualify the native worker or any human, live or release gate.

## Public authorization inspection for native campaign authoring

Decision 0069 adds `setup-inspect-authorization` as a read-only, repository-bound
observation. It returns a content-minimized digest and byte count for a held
no-follow external record, after checking the clean exact HEAD, committed task
source and configuration/record physical distinction. This CLI-only increment
left the native form reading the record in-process; the subsequent native
worker increment below removes that render-thread path. The command by itself
does not satisfy Task 36.3.2.2. The exact-source CLI unit
target passes 21/21 and disposable campaign Setup passes 7/7, including
bounded record inspection, stale repository/head/task-source bindings,
configuration hard link, parent alias, symbolic record link, malformed and
oversized bytes, and a repository-contained record. Strict workspace Clippy
with warnings denied passes. The source/schema-reviewed inventory regenerates
from 1,783 to 1,784 public items and retains 825 explicit gaps: one new public
Rust surface, no removed surface or changed applicability, four changed
heuristic test mappings, and no normalized gap change. A mapping is not proof
of coverage. The full Python suite passes 43/44; its sole failure is the
unchanged CM-R01.6 eight-input source-bound freshness hold. No package,
receipt, binding or digest was renewed. Cargo formatting, documentation,
architecture, no-write inventory and staged whitespace checks pass. An
independent exact-commit review of
`3470b4f682e0de377bd13b61725f36ae0d1189e4` returned PASS without
findings. The reviewer reproduced a positive bounded observation, wrong
repository/head/task-source refusals, physical aliases, links, oversized input
and a concurrent record replacement. The review leaves the native worker,
qualified-human, live-provider and release boundaries open.

## Native campaign Setup worker increment

Decision 0070 routes the native form through the reviewed read-only inspection
and public campaign writer on the eight-slot bounded worker. The submitted
form and diagnosed repository/head/task-source binding are snapshots. The
worker rejects a malformed or foreign inspection receipt, builds and verifies
the candidate from its digest, caps the private candidate stdin at 1 MiB and
checks the public write receipt against the exact candidate byte count and
SHA-256. The UI retains both submitted argument vectors for Show-command,
disables form edits while pending, and accepts only a matching request and
current diagnosis before selecting the result. When either exact command is
unavailable, the write button is disabled while the fields stay editable.
A failure or uncertain effect
requires destination inspection rather than an automatic retry. Opening the
form grants no campaign authority, and the writer itself rechecks the source
and record at publication.

The real-process native Setup test covers the successful inspection/write path
and selected campaign identity. A delayed synthetic inspection regression
changes the form after submission, checks the submitted command preview,
returns a foreign receipt and proves the writer was never invoked. The first
Setup no-diagnosis fixture also checks that the write button cannot act before
an exact command is available. The first focused native run failed because its
former synchronous assertion did not wait for the worker; the test now waits
for the exact result. A first strict
Clippy run found a missing terminal semicolon; it was corrected. The retained
failed outputs are private and are not counted as final-source passes.

The first native all-target run used a precompiled end-to-end verification
fixture with the same two-frame assumption. That one fixture failed on the
honest pending state; all preceding native targets in the run passed. The
verification fixture now waits for the confirmed worker result. Its focused
real workflow passed 1/1. The first cumulative rerun passed 177/177 tests
across 14 native targets, including Setup 6/6 and verification 7/7, before
the exact-command disabled-state correction above. It is preliminary
regression evidence relative to the corrected source. The corrected Setup
target passes 6/6, and the exact current-source native all-target suite passes
177/177 across 14 targets, including verification 7/7. Strict workspace
Clippy with warnings denied also passes. The full Python suite ran 45 tests:
44 passed; the sole failure is the unchanged CM-R01.6 eight-input `input-drift`
source-bound freshness hold. No package, failed receipt, binding or digest
was renewed. Formatting, documentation, architecture, no-write inventory and
staged whitespace checks pass. The source/schema-reviewed inventory has 1,788
public items and 825 explicit gaps. Four public UI APIs were added, no surface
removed, one existing Setup applicability/mapping changed, and the normalized
gap set did not change. These heuristic mappings are not coverage proof.
Independent review of `0873af470aaf9cc0f4905da0eb581ddd3c832301`
returned FINDINGS with one High item: the UI selected a pathname after
receipt validation, so a concurrent same-ID authority replacement could be
reported as the confirmed write. The private report remains unedited and its
scoped tests pass; that verdict does not approve the correction. Configuration
and export actions still use in-process writers, and the complete state/depth
catalogue remains open. At this reviewed commit, the path-based campaign
selection remained an in-process UI read. No runtime-provider or human
qualification is claimed.

## Native receipt-to-selection correction

Decision 0071 changes the confirmed-write selection path to open a held
no-follow regular file, bind its size to the receipt, read at most that size
plus one byte, compare the buffer SHA-256 with the receipt, and parse and
select from those same bytes. A mismatch clears selection and asks the owner
to inspect the destination. Manual path selection and coordinator revalidation
of later actions remain separate. A disposable real-process test holds a
successful writer response, atomically substitutes a valid same-ID document
with changed authority and requires refusal without a success message.
The source/schema-reviewed inventory regenerates from 1,788 to 1,790 public
items with 825 unchanged explicit gaps: two new selection APIs, no removed
surface, no changed applicability or heuristic test mapping for an existing
item, and no normalized gap change. Existing campaign-selection category
requirements are pinned against nearby-text drift. Documentation,
architecture, formatting, no-write inventory and whitespace checks pass.
The focused real-process replacement test passes 1/1. The exact-source
native UI all-target suite passes 178/178 across 14 targets, including Setup
7/7 and verification 7/7. Strict workspace Clippy with warnings denied
passes. The first focused compilation failed on an owned `unused_mut`
warning; it was corrected before the successful focused and cumulative
tests, and the failed output is retained privately. The full Python suite
passes 45/46, with only the unchanged CM-R01.6 eight-input source-bound
`input-drift` hold. No package, failed receipt, input binding or digest was
renewed. All heavy tests/builds used one Cargo job, one Rust test thread and
the shared build slot. Exact-commit independent re-review is pending; Task
36.3.2.2 and all human, live-provider and release gates stay open.
