# Campaign specification read correction

The independent read-only review of `6a50b5dfc3f1bdbb05f5e2c996a98e8e86b2eab3`
found that native campaign selection could follow a file replaced after a
picker snapshot. That review's High finding remains open until the native
selection command and response binding are implemented and independently
re-reviewed.

This first, backend-only correction changes the shared campaign specification
loader. It holds a no-follow descriptor, checks ordinary-file identity before
and after the bounded read, and parses those held bytes. A deterministic
ordinary-file-to-link replacement is refused. This is builder verification,
not independent acceptance or live campaign qualification.

## Backend checkpoint verification

The deterministic replacement case passed 1/1 and the complete
`codingmage-campaign` all-target suite passed 48/48 on this source. Strict
workspace Clippy with `-D warnings`, `cargo fmt --all -- --check`,
`docs_check.py`, `check_architecture.py` and diff whitespace passed. The full
Python suite ran 49 cases; 48 passed and the sole failure remains the same
CM-R01.6 eight-input source-bound `input-drift` hold. No binding, digest,
package or failed receipt was renewed. Original failure details remain in
private test output. The inventory generator was reviewed before `--write`;
its 1,835 items and 826 explicit gaps retain the same normalized public API,
applicability and gap sets. Changed source locations and heuristic test-name
suggestions are not accepted coverage proof.

CM-R01.6 source-bound evidence renewal, human and live gates remain open.

## Native coordinator selection correction

ADR 0081 adds a public, bounded `campaign-select` snapshot. The native
campaign file click, typed path and remembered path now use the worker command.
The remembered path waits for `doctor` to identify the opened repository.
Responses must match the exact selected path, configuration, observed
repository, generation and request ID; unsupported, malformed, stale and
cross-project replies cannot select a campaign or enable status observation.
The UI shows the selection state and exact Show command. A synthetic fixture
first listed an ordinary campaign file, then replaced that name with a link;
the real coordinator command refused it, and the native selection retained no
campaign or status. The in-process guided campaign write receipt path remains
to be moved off the render thread under open Task 36.3.2.2.

An attempted new selection with stale diagnosis clears the prior in-memory
campaign and its observations. When a diagnosis refresh was in flight, its
replacement is queued before selection; a failed refresh prevents the
snapshot from being adopted. A recovered diagnosis leaves the attempted new
campaign unselected until the person retries.

The first combined CLI/UI all-target run passed its CLI targets, including
real campaign mission and project-open workflows, and then failed all seven
native execution fixtures: they advanced to campaign actions before asynchronous
selection returned. The failed output is retained privately. After those and
other immediate-selection fixtures were corrected, the focused execution
target passed 7/7. The complete native UI all-target suite passed 203/203
across 14 targets after the stale-selection clear was added. It includes
real-process campaign selection, reports, readiness, setup, keyboard,
software rendering and recovery. A subsequent small diagnosis-request
ordering correction and stronger recovery assertions passed the final-source
campaign target 15/15. The earlier full native suite did not contain those
last few lines; the final-source focused target verifies their boundary.

The inventory source and schema were reviewed before regeneration. The
result has 1,837 items and 826 explicit gaps: three new normalized public
APIs, one removed direct loader API, no changed applicability for an existing
API and the same gap set. Source-line IDs and suggested test-name mappings
shifted; those mappings are not accepted coverage evidence. Final strict
workspace Clippy with `-D warnings` passed on the final source after
extracting one overlong report fixture helper; that helper's focused report
target passed 7/7 before the later campaign-only ordering correction.
The full Python suite ran 49 cases; 48 passed and the only failure is the
unchanged CM-R01.6 eight-input source-bound `input-drift` hold. No binding,
digest, package or original failed receipt was renewed. Formatting,
documentation, architecture, inventory no-write and diff-whitespace checks
pass on this batch. Independent re-review of the earlier High finding and
the wider human/live gates remain open.
