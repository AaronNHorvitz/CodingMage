# Native source-bound Reports export

Scope: a Task 36.3.2.2 increment for the Reports export action. This is local
implementation evidence, not an acceptance criterion or a completed state catalogue.

The button builds the same `codingmage report-export` argument vector that its
in-context Show command presents: selected configuration, selected campaign,
destination, path inclusion and overwrite. The public coordinator reconstructs a
schema-one report from authorized repository and durable campaign records. The native
app does not send its schema-three observation JSON as authority. That JSON remains
visible under an explicit local-observation label, since its source timing and schema
can differ from the exported document.

The bounded UI queue dispatches export on a separate supervisor. A second writer is
refused while the first process remains unresolved. The normal control worker and
window shutdown do not join a stalled export. Selection generation and exact request
identity reject stale, duplicate and foreign responses. Success requires a strict
schema-one receipt matching campaign, repository and privacy choice, with a nonempty
written result. Invalid receipts and timeouts leave the destination uncertain for
the operator to inspect. The coordinator writer retains its same-mount containment
and explicit no-overwrite default described in Decision 0045.

The first focused real-coordinator Reports run exposed three assertions that still
expected the old helper's refusal wording or stale UI-observation text in the
exported file. The original 3/6 failure is retained privately. The corrected
focused Reports target passed 6/6 on disposable repositories with fake providers:
repository and linked destinations were refused; outside export, privacy opt-in
and overwrite worked; stale local observation text was absent from the fresh
source-bound file. A strict receipt unit checks foreign identity, wrong schema,
false/empty result, wrong privacy choice, malformed JSON, duplicate and unknown
fields. A worker regression checks control response, finite timeout and prompt
window-worker shutdown while a source report command stalls.

The first cumulative native all-target run passed 135 tests and failed one
end-to-end assertion that still expected schema-three `delivery` text in the
source-bound schema-one export. Its original failure is retained privately.
The assertion now checks schema version, selected campaign, recorded status,
privacy and the explicit delivery limitation; the corrected focused workflow
passed 1/1. The first strict workspace Clippy run found similar local names
and an overlong supervisor function. After extracting the bounded source
runner, strict workspace Clippy passed with warnings denied. The exact-source
software-rendered native UI all-target suite passed 142/142 across 14 targets,
including the real-coordinator Reports 6/6, the corrected end-to-end workflow,
the strict receipt cases and the stalled-command control/shutdown regression.
The Python unittest suite ran 42 tests: 41 passed, and the sole failure was the
unchanged CM-R01.6 eight-input source-bound evidence freshness binding. No
digest was renewed. Final `cargo fmt --all -- --check`, `scripts/docs_check.py`,
`scripts/check_architecture.py`, no-write verification inventory and whitespace
checks passed. The generated verification inventory is a heuristic index: its
1,764 items and 825 explicit gaps do not prove coverage, and enum variants such
as the new worker job are not separate indexed items. The regenerated file has
29 moved line-derived IDs and 113 heuristic test-list mapping changes, with no
added or removed named surface, applicability-category change or explicit-gap
change. This is no independent coverage gain.
The section-five state/depth/action catalogue, source-backed inline report inspection,
installed desktop and assistive-technology checks, frozen human trials and live
provider qualification remain open. No acceptance criterion or gate is ticked.
