# Sprint 36.3 exact command preview increment

This is an incremental implementation under open Sub-task 36.3.2.2 and Decision 0023. It does
not complete the native UI state catalogue or the "Show command" rule for all controls.

The top-bar **Refresh diagnosis** action and Campaign **Run preflight** action now build their
argument vectors in one method each. The same vector is submitted to the sibling `codingmage`
worker and shown, on request, in a collapsed **Show command** section beside the action. The
preview includes the resolved executable path, preserves argument boundaries with POSIX quoting,
and never submits or copies the command. Non-UTF-8 paths or invisible controls disable the matching
action and explain why no exact command is available. The app does not display the executable path
until the **Show command** section is expanded.

The previous "Refresh (F5)" button label was inaccurate: the button refreshed diagnosis only,
while F5 also refreshes a selected campaign. The button is now labelled **Refresh diagnosis**;
F5 remains a separate broader shortcut whose exact command list is still open work.

This slice adds no new backend operation or authority. Direct local Git reads, run-record scans,
configuration and process-observation work, plus controls that create request IDs, remain open
under Sub-task 36.3.2.2. Opening the preview does not admit or start a campaign.

## Verification disposition

The source tree containing this file was checked before its main commit. Focused tests compare
the shown `doctor` and `campaign-preflight` text with the argument builders on disposable
repositories; the preflight test uses fake providers and never claims live qualification. A
missing sibling executable still reaches the existing typed diagnosis failure in the automatic
observation path; the clickable Refresh action is disabled without an exact preview.

| Check | Result |
| --- | --- |
| Sibling CLI build, then `cargo test --locked -p codingmage-ui --all-targets -- --test-threads=1` | 79 passed, 0 failed, including hostile work-plan titles, command previews and the missing-coordinator regression |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | Passed |
| `cargo fmt --all -- --check`, `python3 scripts/docs_check.py`, `python3 scripts/verification_inventory.py`, `git diff --check` | Passed; inventory contains 1,679 surfaces and 811 explicit gaps |
| Python unittest suite | 41 passed, 1 retained CM-R01.6 evidence input-drift failure on the same eight bound inputs |

The heavy commands used the shared build slot with one Cargo job and one test thread. The UI
all-target run used a labelled CPU software renderer. An earlier candidate run exposed the
missing-coordinator regression and was retained as a failure; the corrected focused shell test
passed before the successful cumulative run. Independent re-review of the correction remains
open. The remaining direct UI repository and state reads keep Sub-task 36.3.2.2 open.
