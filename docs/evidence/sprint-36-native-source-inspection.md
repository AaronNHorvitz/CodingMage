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
without findings. This increment needs its own exact-commit review. Installed
Wayland/X11 and Orca, clean desktop, frozen user trials, real providers and
whole-product completion remain unqualified. No task, acceptance criterion
or gate is ticked for this increment.
