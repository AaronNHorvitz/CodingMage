# Native source-bound report inspection

Scope: a local Task 36.3.2.2 increment. The exact-commit independent review,
whole screen catalogue, desktop qualification and human/live gates remain
separate.

The Reports screen now shows an explicit read-only source inspection action,
its exact `campaign-outcome-report` command and a bounded inert snapshot of
the returned schema-one JSON. Inspection requests changed-file paths to be
omitted. The response must match the selected campaign, repository, authority
digest and initial commit; an incorrect schema, unexpected field, invalid
path-privacy shape or malformed response is refused. A selection change clears
the snapshot, and the action never starts a campaign. The command runs on its
own bounded supervisor, leaving the normal coordinator control worker and
window shutdown responsive even if inspection stalls.

The source-bound export action no longer waits for the UI's local schema-three
assembly. Its exact displayed `report-export` command asks the coordinator
to read current records and publish through the guarded writer. Local
observation freshness remains visible and is not claimed to be part of that
fresh export. A source snapshot is labelled as an earlier explicit read until
the owner requests another one; a timeout or malformed result never claims
success.

Focused `codingmage-ui` Reports tests use disposable repositories and the real
coordinator binary with fake providers. They cover the exact command, bound
path-private inspection, no repository mutation, selection clearing, malformed
shape, oversized response, limited preview, duplicate request, and source
export after a stale local observation. A worker test covers both stalled
source export and stalled inspection while a control runs and the window
worker shuts down promptly.

The first focused attempt failed compilation because a Unix permissions trait
import remained in the test wrapper after the helper extraction. The corrected
focused library passed 69/69. The first Reports integration attempt passed
6/7; its new assertion matched the campaign name in both the shell and the
exact command. The corrected focused target passed 7/7. The first cumulative
native run reached Reports and passed 5/7 there before two UI assertions
failed: one expected the superseded local-freshness sentence and one expected
the local JSON below a newly added source/export section to remain in the
visible viewport during a concurrent refresh. The corrected focused run
passed 70/70 library tests and 7/7 Reports tests. These failed attempts are
retained in the private session record; they are not counted as passing gates.

Final-source focused `cargo test --locked -p codingmage-ui --test reports
--lib` passed 70/70 library and 7/7 Reports tests. The software-rendered
native `cargo test --locked -p codingmage-ui --all-targets` passed 145/145
tests across 14 targets. Strict `cargo clippy --locked --workspace
--all-targets -- -D warnings` passed. The Python unittest suite ran 42
tests: 41 passed and only the unchanged CM-R01.6 eight-input source-bound
freshness binding failed; no digest was renewed. Formatting, documentation,
architecture, no-write inventory and whitespace checks passed. The inventory
generator was inspected against its version-one output before regeneration:
1,767 items and 825 explicit gaps, three new public UI methods, no removed
semantic surface or change in category applicability or gap count. Thirty-two
line-derived identities moved and 390 crate-wide, capped keyword mappings
changed because the new negative test entered the candidate list; this is
not independent coverage gain. The full commit identity
and independent-review request are recorded in the private handoff.

The parent source-bound export commit received a bounded independent PASS
without findings. This increment's corrected source later received the bounded
independent PASS described below. Installed
Wayland/X11 and Orca, clean desktop, frozen user trials, real providers and
whole-product completion remain unqualified. No task, acceptance criterion
or gate is ticked for this increment.

## Independent review finding and correction

The independent exact-commit review of `a4466b52f6ffc8a27155f6d27efcf74f84d78557`
returned **FINDINGS**. It demonstrated that duplicate `changed_files` keys in
a nested untyped value could pass the path-privacy check after JSON collapsed
the first value, while the unvalidated original response bytes were still
shown as a successful snapshot. The finding also covered unexpected nested
path-bearing members. The report is retained privately and is not overwritten
or counted as approval.

The correction rejects duplicate keys at every JSON object depth before any
map value is collapsed; parses status, blockers, final report, changes and run
records against closed typed projections; checks selected identity, schema and
changed-file privacy; and serializes only that validated model for the inert
preview. A malformed result clears a prior successful snapshot and leaves a
visible failure. Regressions cover duplicate changed-file keys ending in null,
extra nested path-bearing keys, duplicate keys in each nested projection,
missing required projections, prior-snapshot clearing and active source reads
through the real coordinator with fake providers. This is a correction to the
local implementation, not a new live or human qualification.

The corrected-source focused `cargo test --locked -p codingmage-ui --test reports --lib` run passed
71/71 library tests and 7/7 disposable real-coordinator Reports tests. The
new prior-snapshot-clearing regression passed separately. The first cumulative
native run passed 146/146 across 14 targets. Strict Clippy initially failed
two style checks in the correction (backend-mirror booleans and owned parser
input); those were corrected without changing the report contract, with the
failure retained privately. Final-source strict workspace Clippy with
`-D warnings` passed, and the final-source software-rendered native UI
all-target run passed 146/146 across 14 targets. The Python unittest suite
ran 42 tests: 41 passed and only the unchanged CM-R01.6 eight-input
source-bound freshness test failed; no digest was renewed. Formatting,
documentation, architecture, no-write inventory and whitespace checks pass.
The reviewed inventory generator produced 1,767 items and 825 explicit gaps:
zero added or removed semantic public items, zero applicability changes,
16 line-derived ID moves and 400 capped crate-wide heuristic test-mapping
changes. This mapping movement is not coverage gain. Two read-only exact-SHA
re-review attempts of the corrected commit found no new source issue, but
neither acquired the shared heavy-build reservation for independent focused
Reports tests or strict Clippy. Both verdicts remain **INCONCLUSIVE** as
historical records. The later exact-SHA review of
`e4b846567f829e06343f8dca0ffbe9d062226318` independently ran the focused
Reports workflow, recursive duplicate-key regressions and strict workspace
Clippy and returned **PASS** without findings. It also reviewed the Work plan
increment at that SHA. All human, desktop, live and release gates remain open.
