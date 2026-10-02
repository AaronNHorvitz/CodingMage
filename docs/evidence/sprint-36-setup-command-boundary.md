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
the shared build slot. Bounded independent exact-commit re-review of
`0fcbb1e43eb7d1d1874b69f19b58c748923b9162` returned PASS without
findings for this correction. Task 36.3.2.2 and all human, live-provider and
release gates stay open.

## Public guided Setup export and native worker

Decision 0072 adds `setup-export-copy` to the sibling coordinator. The command
requires an observed repository ID, selected configuration, exact source and
outside-repository output. It accepts the selected configuration or a parsed
campaign document bound to that repository and the selected semantic authority
SHA-256. It reads at most 1 MiB through a held no-follow regular-file
descriptor and rechecks source identity and digest at the guarded publication
boundary. The existing public writer keeps create-only, explicit overwrite
and uncertain-write recovery semantics.
Protected source/configuration path and inode aliases cannot be replaced.
The JSON receipt contains the repository ID, byte count and SHA-256, without
document contents.

The native Setup export action accepts only the opened configuration or
currently selected campaign and submits the displayed command to its
bounded worker. It freezes the submitted arguments and destination while
pending, disables duplicate actions, rejects stale selection or response
identity, and compares a held no-follow output read and final named inode
to the receipt before showing success. A changed destination yields
inspection guidance. The configuration writer and remaining selection reads
still run in-process; the section 5 state/depth catalogue and Task 36.3.2.2
remain open.

The disposable real-process command tests cover configuration and campaign
copies, content-minimized receipts, no-overwrite and explicit replacement,
foreign/malformed/oversized source data, a same-repository changed campaign
authority, wrong repository identity, source inside the repository and a
protected output hard link. Native tests cover a real public export with
Show-command, an atomic destination replacement after a successful coordinator
receipt, and a malformed coordinator receipt.
These tests are local boundary evidence, not installed desktop, human or
live-provider qualification.

The source and version-one inventory generator were reviewed before writing
the updated public inventory: 1,790 to 1,794 items, 825 explicit gaps before
and after. Four new crate-visible export APIs were added; no public surface
was removed. The existing `export_document` boundary applicability gained a
category, 244 existing heuristic test mappings changed as test names entered
their crate-wide candidate lists, and the normalized gap set is unchanged.
Those mappings do not certify test coverage.

The preliminary combined CLI/native all-target batch passed 70 CLI tests
with two sustained qualification cases ignored, then reached a native Setup
fixture whose compiled version still assumed synchronous export. That one
fixture failed; its original output is retained privately. The updated
fixture waits for the worker result and passes in the final-source Setup
target. The combined run was compiled before the final campaign-authority
and UI response changes, so it is regression context, not a final-source
all-target pass. Exact final-source CLI Setup tests pass 9/9, native Setup
tests pass 10/10, and the public command-help regression passes 1/1.

The native all-target suite on the campaign-authority and response source
passed 181/181 across 14 targets, including Setup 10/10 and verification
7/7. Strict workspace Clippy first rejected an owned `nonminimal_bool`
form in the selected-source guard. The equivalent positive predicate fixed
the lint; strict workspace Clippy with warnings denied then passed. The
failed lint output is retained privately. The affected Setup target passed
10/10 on the exact post-lint source. Diff inspection then added an exact
configuration-byte check at publication for campaign exports and a final
named-inode recheck after the native output read. On this final source, the
real-process CLI Setup target passes 9/9, native Setup passes 10/10, and
strict workspace Clippy with warnings denied passes. Cargo formatting,
documentation, architecture, regenerated inventory freshness and diff
whitespace checks also pass. The full Python suite ran 46 tests with one
failure: the unchanged CM-R01.6 eight-input source-bound `input-drift` hold.
No package, failed receipt, binding or digest was renewed. All heavy checks
used the shared build slot with one Cargo build job and one Rust test thread.
Its first independent exact-commit disposition is documented below.

## Native Setup export lifecycle correction

The read-only independent review of
`a1e6b785414fb3a2a9ac5cbf2c92d79285097510` returned FINDINGS/High:
the general native worker cancels its child after project or campaign
selection changes and on window close. That can kill an export after public
publication but before its result reaches the interface. The original
review report is retained privately. It is not converted into a pass.

Decision 0073 records the correction. Setup persists a create-only private
intent before dispatch. The submitted intent digest is carried to a separate
native helper, which checks the persisted bytes, holds a private intent lock,
runs the existing public coordinator command within its deadline and writes a
bounded terminal record before exiting. The worker's supervisor is detached
from UI cancellation once started. The current selection still rejects stale
presentation. Reopening the exact project uses the recorded result and
source-bound public receipt to verify the held destination bytes and named
inode before showing success; absent or malformed state stays uncertain.
The owner may inspect the destination and clear a notice after execution has
released the lock, including when a queued helper never started. A late queued
helper then fails its original digest check rather than starting the command.

The disposable held-command regression covers project close and campaign
reselection both before publication and after publication but before receipt
delivery. It checks helper survival, terminal-record retention, stale-response
rejection, lock refusal during execution and verified recovery after reopen.
The private-intent unit test rejects modified bytes after submission. A guided
Setup fixture now explicitly inspects and clears a failed export notice before
retrying. These are native local workflow checks; they do not perform installed
desktop, screen-reader, real-provider, independent acceptance or release work.

The version-one inventory generator and source/schema changes were reviewed
before regeneration. It now records 1,806 items versus 1,794 at the reviewed
parent and retains 825 explicit gaps. Twelve normalized API entries were
added for private export/recovery helpers; no normalized entry was removed.
An unrelated `Job::label` boundary category had changed only because a nearby
new deadline comment entered its heuristic context, so its prior applicability
was pinned in the generator. Existing normalized applicability, heuristic
mapping suggestions and gap counts then remained unchanged. These suggestions
do not certify coverage.

The native all-target suite passed 183/183 across 14 targets on this correction
before a lint-only function extraction. The final extracted source passed the
affected library target 96/96 and Setup integration target 11/11, including the
four held-command cases. Strict workspace Clippy with warnings denied passed
after correcting the owned function-length, match-form and test-placement
lints; the first failed lint output remains in private state. The full Python
suite ran 46 tests and passed 45. The sole failure is the pre-existing CM-R01.6
eight-input source-bound `input-drift` hold; no package, failed receipt, binding
or digest was renewed. Every heavy build/test used the shared build slot, one
Cargo job and one Rust test thread. This correction awaits its own independent
exact-commit re-review.

An additional process-level regression launches the native export helper from
a short-lived shell process, waits until the public coordinator is held, then
lets that launcher exit. The helper still publishes through the coordinator
and writes its private terminal result after the launcher is gone. This is a
local Linux process-lifetime test, not an installed desktop window test. It
passed 1/1; the final Setup target passed 12/12 and strict workspace Clippy
passed again. The first attempt at this test failed compilation on an owned
fixture move, was corrected, and its output is retained privately.

The independent exact-commit review of
`8f04a6c19d64b12b4caa860b7fc19a951f541b15` returned a bounded PASS for
the lifecycle correction. It also reported a Low retention gap: clearing an
intent left its per-request terminal file behind, so repeated exports could
accumulate private recovery files without a ceiling. The follow-up removes
the exact regular terminal file under the intent lock before removing the
intent and syncs the private directory after each removal. An interrupted
clear thus leaves an unresolved intent instead of an orphan result. A
disposable native Setup regression checks that two successful exports leave
no terminal files; another checks that inspected explicit clear after a
failed receipt removes its terminal file. On the correction source, the
native UI all-target suite passes 184/184 across 14 targets and strict
workspace Clippy with warnings denied passes. Formatting, architecture,
documentation and whitespace checks pass. The reviewed version-one inventory
remains at 1,806 items and 825 explicit gaps; only four location IDs moved,
with no normalized API, applicability or candidate test-mapping change. The
full Python suite passes 45/46: its sole failure is the unchanged CM-R01.6
eight-input source-bound `input-drift` hold. No binding, package, failed
receipt or digest was renewed. All heavy checks used one Cargo job, one Rust
test thread and the shared build slot. This follow-up awaits its own
exact-commit review. Task 36.3.2.2 and all human, live and release gates stay
open.
