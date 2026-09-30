# Native report export worker

Task 36.3.2.2 remains open. The Reports screen now queues an explicitly requested
outcome-report snapshot on the existing bounded UI worker. The request captures the
destination, repository, path-privacy and overwrite choices. The UI shows a pending
state, rejects a duplicate request and accepts a result only for the current selection
generation, repository/campaign binding and request identity. The existing directory-
handle writer and its privacy/overwrite checks run on that worker. Changing the selected
view cannot relabel a stale export response as current success. An already started write
can still finish because it was explicitly requested.

The focused disposable real-coordinator Reports target passed 3/3 with fake providers.
It covers export refusal inside the target (including a linked parent), privacy opt-in,
overwrite refusal and consent, a duplicate request while pending, and a completion with
the wrong request identity. The stale-status case confirms the exported report retains
its observation warning. This is local fake-provider and deterministic evidence,
not an installed-desktop, screen-reader, human or live-model qualification.

The first cumulative native UI run failed one existing end-to-end workflow test: it
read the exported file immediately after the new asynchronous request, before the
worker result arrived. That failed run remains retained. The workflow now waits for
the matching completion before snapshot and file inspection. The corrected focused
end-to-end workflow passed 1/1 under software rendering. The exact corrected-source
native UI all-target suite passed 128/128 across 14 test targets under software rendering.
The first strict workspace Clippy run then found that the expanded Reports integration
test exceeded the 100-line function limit. Its duplicate and foreign-completion
assertions were extracted into a helper with no production change. Corrected strict
workspace Clippy passed with warnings denied. The exact final-tree native UI all-target
suite then passed 128/128 across 14 test targets under software rendering, including
the real-coordinator Reports and setup-to-outcome workflows.

`cargo fmt --all -- --check`, strict `cargo clippy --locked --workspace --all-targets
-- -D warnings`, `python3 scripts/docs_check.py`, `python3 scripts/check_architecture.py`,
the no-write inventory check and diff whitespace check passed. Every heavy batch used
the shared build reservation with one Cargo job and one test thread. The Python
unittest suite ran 42 tests: 41 passed and only the pre-existing CM-R01.6
source-bound eight-input drift check failed. No package or evidence digest was
renewed. The first cumulative and Clippy failures above remain retained rather than
being counted as passing checks.

The verification inventory was regenerated from its unchanged extractor and schema:
1,744 items and 825 explicit gaps. Three UI method entries were added; 36 line-derived
IDs, three heuristic applicability labels and three test-list mappings changed. This
index is not a coverage or independent-review result.

The report assembly and on-screen JSON preview still run during rendering. The export
has no equivalent sibling `codingmage` command, so section 2 and the every-action
Show-command rule remain open. The same-user rename and overwrite race limits of
Decision 0038 remain. No acceptance criterion or gate is closed by this increment.
