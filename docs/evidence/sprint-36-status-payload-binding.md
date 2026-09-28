# Sprint 36 campaign payload binding correction

This is a backend-consumer correction needed for truthful status presentation under open Task
36.3.2.2. It does not complete the section 5 screen-state catalogue or qualify the desktop.

The UI already discarded stale response envelopes, but the status and mission payloads were not
compared with the selected campaign after parsing. The new regression first observed a real
`campaign-status` response from a disposable fake-provider campaign. It then changed only the
payload campaign ID in a synthetic response whose envelope still matched the selected campaign.
Before the correction, the UI accepted and displayed the foreign status. The focused red run
failed at that exact assertion; its private receipt is retained outside Git.

The corrected consumer checks campaign ID on non-null status, and both campaign ID and authority
digest on mission status, before storing either value. A mismatch is a contract failure and cannot
drive dependent head/change requests from a foreign status. The earlier valid observation is
marked stale, including a visible stale mission warning, and a later correctly bound response
recovers. A null status still means no durable campaign, while the coordinator's explicit
no-charter response remains distinct.

The original focused test failed 0/1 at the intended assertion: the forged status replaced the
selected campaign's real status. After the correction, the focused test passed 1/1. The corrected
source passed the sibling CLI build, strict workspace Clippy and all 91 native UI all-target tests
under the shared build slot with one Cargo job, one test thread and labelled CPU software
rendering. The Python suite ran 42 tests: 41 passed and only the retained CM-R01.6 source-bound
freshness test failed on its eight documented input-drift paths. No evidence digest was renewed.
Formatting, documentation, architecture, inventory and diff checks passed; the inventory has
1,702 surfaces and 825 explicit gaps, with only line-anchored IDs shifted. Private red, focused
and cumulative receipts are retained outside Git.

This is local implementation evidence. Exact-commit independent re-review is still required.
Human, real-desktop, live-provider, package, release and CM-R01.6 gates remain open.

The exact-commit independent review of `5a8b6d2f6393d205b59290f5f2daf9bf59f20c7d`
returned **PASS** with no finding in the scoped payload-binding change. The reviewer ran the
focused regression 1/1, native UI all-target 91/91, sibling CLI build, strict workspace Clippy
and static checks from a clean archive. Python retained only the same CM-R01.6 eight-input
source-bound freshness failure. This review did not qualify the status-bar follow-up, a live
provider, human desktop behavior, package construction or release.
