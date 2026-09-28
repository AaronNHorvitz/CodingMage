# Sprint 36 Campaign-Head Task-State Projection

Task 36.3.2.2 remains open. This increment removes one direct UI Git read: the campaign-head
task source. `campaign-head-plan` binds a request to the active configuration, campaign and
reconciled full commit ID. It returns only version, identities, task-source SHA-256, sub-task IDs
and literal checkbox states. The UI rejects unknown output fields, duplicate IDs, unsupported
versions and mismatched identities or head. Source text is never present in this projection.

The real disposable-campaign fixture in `crates/codingmage-ui/tests/campaign.rs` checks a
completed unit, source/head distinction, source-free projection, and stale-head, absent-state and
cross-repository refusals. A Git fixture checks exact immutable blob retrieval and held-identity
failure after a head change. A forged stale-head response is rejected by the UI and withheld from
the task overlay. The model unit test checks malformed projections. This does not
qualify a real provider or desktop.

The first cumulative candidate run passed 38 Git library tests, 4 CLI library tests and 81 UI
all-target tests. Strict workspace Clippy then rejected a 112-line `App::handle_response` under
the 100-line function limit; the failed result is retained in the private verification log. The
head-plan response branch was extracted into `campaign_screen::accept_head_plan`. The corrected
Clippy rerun then reached the campaign integration test and rejected its 112-line test function;
its exact failure is retained privately. Projection assertions were moved into a test helper.
Neither earlier run was a passing cumulative gate. The corrected source passed strict workspace
Clippy and all 81 UI all-target tests; the Git and CLI library suites passed 38 and 4 tests
respectively in the first run, and their source was unchanged by the subsequent UI refactors.
Python unittest ran 42 tests: 41 passed and the single retained CM-R01.6 source-bound freshness
test failed on the same eight drifted inputs (campaign/runtime team source, its Python test,
package script, lockfile, README and SECURITY). This increment does not renew that package or
claim a green Python suite. Formatting,
documentation, verification inventory (1,691 surfaces and 816 explicit gaps), and diff checks
pass on the corrected source. The private logs preserve both failed Clippy attempts and the final
run's exact output.

The remaining Task 36.3.2.2 work includes the direct Git change reads, private run-record scans,
the full section 5 state/depth catalogue and exact Show command affordances for the remaining
controls. Independent review and human-only gates remain open.
