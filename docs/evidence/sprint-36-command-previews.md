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

This first slice added no new backend operation or authority. At that checkpoint direct local Git
reads, run-record scans, configuration and process-observation work, plus controls that create
request IDs, remained open under Sub-task 36.3.2.2. Later Decisions 0024 through 0026 moved the
campaign-head task, change and run-record observations through the coordinator. Opening a
preview does not admit or start a campaign.

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

The independent report for `150bca34b59ba4a828dc0ec20b81b8ad870d388c` accepted the three
earlier static corrections but found that the preview accepted U+034F, an invisible Unicode
character. Its verdict was **INCONCLUSIVE**; the reviewer could not acquire the shared build
reservation to reproduce Rust checks. A follow-up source correction now uses a conservative
display-safe predicate shared by both clickable command previews and refuses direct diagnosis
and preflight submission when a selected executable command is unsafe to display. The focused
unit test passed 1/1; an offscreen test with an actual U+034F-named executable passed 1/1 and
observed that neither button nor direct call submitted a request. The corrected candidate tree
passed the sibling CLI build, UI all-target suite (80/80), strict workspace Clippy, formatting,
documentation check, inventory validation (1,680 surfaces / 811 explicit gaps), and diff check.
The Python suite ran 42 tests: 41 passed and the unchanged CM-R01.6 source-bound evidence test
failed on the same eight input-drift paths. The full private gate log is retained outside Git.
This local verification does not independently close the review finding; a fresh exact-commit
review is required. Task 36.3.2.2 and all human/live/acceptance gates remain open.

## Campaign process and control preview increment

The **Start coordinator** button now shows the exact `codingmage campaign --config … --campaign …`
argument vector built by the same function used for its detached process launch. An unsafe or
non-UTF-8 command path disables the action before launch or private launch-file creation.

The four campaign controls prepare display-only idempotency identities beside their buttons and
show the exact `campaign-control` command including `--request`. Expanding a preview neither
records an intent nor invokes the coordinator. Clicking the matching button records and submits
that same identity; if an outcome is uncertain, the next preview uses the existing retryable
identity. The identity is bound to the selected campaign authority and action. A pending control
blocks another request. Cancel still needs two presses and keeps its shown identity between them.
Action-specific preview labels identify which command belongs to which control.

The first focused offscreen test compilation failed because the new test lacked the
`egui_kittest::kittest::Queryable` import; the compiler receipt is retained privately. After the
import was added, the focused fake-provider test passed 1/1, showing the exact start and pause
commands, proving the preview recorded no intent, then proving the clicked pause request used the
same shown identity. On the corrected source, the sibling CLI build and strict workspace Clippy
passed. The UI all-target suite passed 89/89 tests, including start, detach, reconnect,
cancellation, the new preview and software-rendered recovery fixtures. Python unittest ran 42
tests: 41 passed and only the retained CM-R01.6 source-bound input-drift test failed on the same
eight paths. No evidence digest was renewed; Python is not reported as green. The cumulative
receipt is retained privately outside Git. Formatting, documentation, architecture, regenerated
inventory (1,702 surfaces and 825 explicit gaps), and diff checks pass on the final source. The
inventory added six UI functions and removed no explicit coverage gap; line-anchored identities
moved with source lines. These checks are local engineering evidence, not independent,
live-provider or desktop qualification.

This increment is local implementation evidence only. Admission and configuration writes are
still UI-local operations without command equivalents, and other screens still have missing
section 5 states, contextual depth and Show-command affordances. Task 36.3.2.2 stays open.

## Independent combining-mark finding and correction candidate

The exact-commit independent report for `b3f211e67bae531902caedd086e16108c43bbf54`
returned **FINDINGS**. It established that U+0345 passes Rust's `is_alphanumeric()` even though
it is a combining mark, so the prior predicate could enable a command whose path was visually
misleading. The reviewer could not acquire the shared heavy slot for independent Clippy or UI
tests; its static and Python checks ran, with the same retained CM-R01.6 failure.

The correction uses Unicode general categories and refuses every mark category. Unit cases cover
U+0345 in an executable path and U+05B0 in an argument, while keeping ordinary international
letters accepted. An offscreen fake-provider adversarial case supplies marked executable,
configuration and campaign-specification paths in turn; it checks that start and all four control
previews are unavailable and that no launch record or control intent is written. The focused
offscreen test passed 1/1 under the shared build slot. The corrected source passed the sibling CLI
build, strict workspace Clippy and all 90 native UI all-target tests under a labelled CPU software
renderer. The Python suite ran 42 tests: 41 passed, while the same CM-R01.6 source-bound freshness
test failed on its eight documented input-drift paths; no evidence digest was renewed. Formatting,
documentation, architecture, diff and regenerated inventory checks passed (1,702 surfaces, 825
explicit gaps). The private cumulative receipt is retained outside Git. An exact-commit re-review
is still required before treating the independent finding as resolved. The full Task 36.3.2.2,
desktop, human, live-provider, package and release gates remain open.
