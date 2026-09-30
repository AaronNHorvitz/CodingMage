# Isolated native report export correction

Task 36.3.2.2 remains open. The independent review of the exact Decision 0040 commit
`0fd80aad213826d8cda2c5b52e9b38975fb3a912` returned **FINDINGS**: the single
worker could be held by a stalled filesystem export, starving coordinator controls and
window shutdown. The original review is retained privately by the review controller;
this correction does not relabel that commit as passing.

The UI now dispatches each immutable report request from the coordinator worker to a
separate bounded supervisor. A one-shot `codingmage-ui` helper owns the unchanged
directory-handle writer. Only one helper may run or await reap at a time; cancellation
and the 30-second deadline signal the child without joining a wedged filesystem call.
The child receives the report and paths through a private, size-limited pipe, with a
cleared environment and no report or repository path in process arguments. The result
remains bound to the original request and current selection.

Synthetic stalled-helper tests exercise a control request while the writer is stalled,
prompt window-worker shutdown, and the finite timeout result. The disposable real-
coordinator Reports target checks inside-repository and linked-parent refusal, privacy
opt-in, overwrite refusal/consent, duplicate request and wrong request identity, and
stale source labelling. These tests use fake providers and software rendering; they are
not real desktop, filesystem-fault, screen-reader, human or live-model qualification.

Verification used `CARGO_BUILD_JOBS=1`, `RUST_TEST_THREADS=1`, software GL and
`bash /tools/build-slot` for every heavy command. The exact final source passed
`cargo build --locked -p codingmage-ui --bin codingmage-ui` followed by
`cargo test --locked -p codingmage-ui --all-targets`: 131 tests across 14 targets,
including the controlled stall, timeout, real-coordinator Reports and setup-to-outcome
workflows. Strict `cargo clippy --locked --workspace --all-targets -- -D warnings` passed.
Formatting, documentation, architecture, no-write inventory and diff-whitespace checks
passed. The regenerated inventory has 1,751 public items and 825 explicit gaps: seven
new UI surfaces, no removed semantic surface or gap. It is an index, not coverage proof.
Twenty-five line-derived IDs, one heuristic applicability label and 400 heuristic
test-list mappings changed, mostly because the UI crate gained named boundary tests;
the 333 stable error-code entries are unchanged.

The first focused Reports run failed two tests because an intermediate implementation
set the shutdown flag on ordinary selection changes. The flag now changes only during
worker destruction; the corrected Reports target passed 3/3. The first strict Clippy
run found eight style issues in the new code; they were corrected before the final
pass. An earlier cumulative suite passed 130/130 before the final size-limit and
failure-guidance refinement; the exact final-source suite above passed 131/131.
These initial failures are retained in the private build logs and are not counted as
passing checks. The Python unittest suite ran 42 tests: 41 passed and only the known
CM-R01.6 eight-input source-bound freshness check failed. No digest or package binding
was renewed.

A timeout may leave a private candidate in the chosen export directory, and same-user
rename races remain.
Report assembly, inline JSON preview, exact public `codingmage` export command and the
wider section-5 state/action catalogue remain open. No acceptance criterion or gate is
closed by this correction.
