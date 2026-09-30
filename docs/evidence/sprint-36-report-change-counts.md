# Report change observation counts

Task 36.3.2.2 remains open. The interface-generated report is now schema 3. Its
`change_coverage` field has an explicit observed flag and separate commit and file truncation
flags; `changed_file_count` is nullable. The screen says "not observed" before the coordinator
has returned changes and "at least N; more omitted" for a truncated list. Exported limits
state when the count is unknown or a lower bound. An observed empty projection remains zero,
and an empty
path list does not claim the document contains repository paths merely because opt-in was set.

The focused unit fixture covers missing, observed-zero and truncated projections, including
the JSON fields and repository-path omission. The disposable real-coordinator Reports test
checks an immediately selected campaign before changes arrive, then checks the observed count
after the coordinator responds. `cargo test --locked -p codingmage-ui report` passed, including
both real-coordinator Reports tests. The exact corrected source passed
`cargo test --locked -p codingmage-ui --all-targets`: 128 tests across 14 binaries under
software rendering. Strict `cargo clippy --locked --workspace --all-targets -- -D warnings`,
`cargo fmt --all -- --check`, documentation, architecture, inventory validation and diff
whitespace checks passed. Every heavy check used the shared build reservation with one Cargo
job and one test thread. The first strict Clippy attempt failed on excessive report booleans
and an overlong assembler; the corrected source groups coverage and extracts run summaries.
The original failure is retained in the private checkpoint. The full Python unittest suite
ran 42 tests: 41 passed and only the existing CM-R01.6 source-bound eight-input drift failed;
no evidence binding or digest was renewed.

The regenerated verification inventory has 1,741 items and 825 explicit gaps. It adds one
public `ChangeCoverage` type; seven line-derived IDs, two context-keyword categories and 188
heuristic test mappings changed. These mappings are an index, not a coverage or independent
review result. This is fake-provider local evidence, not a real-model, independent,
screen-reader or human result.

Report export still performs local file I/O on the UI thread and lacks an exact sibling
`codingmage` command. Concurrent malicious destination changes remain outside the deterministic
directory-handle checks of Decision 0038. These are open native specification gaps, not closed
by the schema correction.
