# Sprint 36 blocker explanation payload binding

Task 36.3.2.2 remains open. The UI already discards responses whose request generation or
repository/campaign binding differs from the current selection. This increment checks the
identity **inside** a successfully parsed `campaign-explain-blocker` payload before accepting
it. A payload naming another campaign now produces an authority-mismatch contract error and
retains the earlier explanation as stale. It cannot replace the selected campaign's explanation.

The focused `codingmage-ui` campaign regression injects a valid explanation for the selected
campaign, then a different campaign ID and blocker code on the same current request binding.
It asserts that the original value remains and the authority-mismatch error is recorded. The
first test draft waited for a non-existent durable explanation on a never-started fixture and
timed out; the corrected synthetic boundary fixture passes. This test does not claim live
provider, human, desktop, or full state-catalogue qualification.

On the final source tree, the focused test passed 1/1 and the software-rendered native UI
all-target suite passed 106/106. Strict workspace Clippy, `cargo fmt --all -- --check`,
`scripts/docs_check.py`, `scripts/check_architecture.py`, deterministic verification inventory
and `git diff --check` passed. The inventory contains 1,725 public items and 825 explicit gaps:
one new public accessor, no removed surface or changed applicability/test mapping, and 25
line-derived ID shifts. Python unittest ran 42 tests with the sole prior CM-R01.6
`test_multi_agent_evidence_binding_is_current` failure on its same eight source inputs; no
source-bound digest was renewed. Cargo and Python work used the shared `/tools/build-slot`
reservation with one Cargo build job and one Rust test thread. A dedicated searchable Blockers
destination and its recovery guidance are still open.
