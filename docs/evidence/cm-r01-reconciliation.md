# CM-R01 Reconciliation of Native UI Source and Evidence Freshness

- **Status:** Local reconciliation and repair; independent review, live qualification and
  package renewal remain open
- **Work package:** CM-R01 in [TASKS.md](../../TASKS.md), capabilities CAP-01, 02, 11, 12, 30,
  39, 45 and 46 in [the capability roadmap](../../CAPABILITY-ROADMAP.md)
- **Prior evidence reused:** [native UI local evidence](sprint-36-native-ui-local.md),
  [native UI verification](sprint-36-native-ui-verification.md) and
  [the human-only register](sprint-36-human-only.md)
- **Retained failed receipts:** `cm-r01/multi-agent-evidence-binding-drifted-7bb3dbf.json` and
  `cm-r01/freshness-failure-7bb3dbf.log`

## CM-R01.1 - Native UI source and test reconciliation

The twelve implementation commits `339d17a` through `8ea475c` on `build/native-ui` are preserved
unchanged. Their tests run offscreen on the Mesa lavapipe software adapter with fake providers
and the workspace `codingmage` binary. The table maps that work to the new capability rows. A
row marked "fake-provider" is deterministic local evidence only; a row marked "human-only" names
the register item that must still be performed by a person or an authority the worker lacks.

| Capability | Reused implementation | Local evidence | Still open |
| --- | --- | --- | --- |
| CAP-01 Native workspace | `crates/codingmage-ui` shell, work plan, campaign, changes and reports screens (Sub-tasks 36.1.1.2 to 36.2.2.2) | Offscreen workflow tests, empty/loading/failure/stale screens from real backend records | Conversations and research screens have no backend contract yet; H1 real desktop launch |
| CAP-02 Setup and connection doctor | Readiness checks and real `doctor` preflight (Sub-task 36.1.3.2) | Missing binary, invalid configuration, unavailable provider and stale state are distinct actionable failures | Runtime socket/model/disk prerequisites belong to the runtime owner; only the coordinator preflight exists here |
| CAP-11 Verification receipts | Test and gate records read from durable state (Sub-task 36.2.2.1) | Records show exact commit, command, exit status and counts from the journal | Receipt renewal for this repository's own evidence is CM-R01.5 and CM-R01.6 |
| CAP-12 Inspectable selective changes | Exact candidate change listing from read-only Git (Sub-task 36.2.2.1) | Per-file diffs presented without mutating the target | Accept/reject per hunk with human-edit preservation is not implemented; it needs a coordinator contract first |
| CAP-30 Crash-safe resume | Detach, reconnect and recovery proofs (Sub-task 36.2.1.2) | Coordinator survives window close; reconnect rejects stale generations and mismatched identities | Runtime-side effect replay protection is the runtime owner's; H4 real-provider run |
| CAP-39 Truthful progress and completion | Distinct source-checkbox, verified, accepted, blocked, deferred, stale and unknown states on every screen (Sub-task 36.1.2.2) | Parity tests against the CLI status contract | Independent review state is shown only when the journal records it; H5 |
| CAP-45 Usable Linux distribution | Release build and startup failure paths (`8ea475c`) | Offscreen screenshots, keyboard navigation, window sizes, resource readings | H1, H2 screen reader, H3 clean-desktop install, H6 packaging gates |
| CAP-46 Reproducible evaluations | Verification document with exact commands and retained digests (Sub-task 36.2.3.1) | Fake-provider results labelled as such | Local-model and independent evidence do not exist |

No Sprint 36 checkbox changes here. Sub-task 36.2.3.2, AC 36.1, AC 36.2 and Gates 36.1/36.2 remain
open for H1 through H6.

## CM-R01.2 - The exact evidence-freshness defect

`docs/evidence/multi-agent-evidence-binding.json` binds the multi-agent local evidence
`codingmage.multi_agent.local.v1` to source commit `319de727917928057c04d7da0a975706483623ef`
and package `8031a62b89e01a35bf8b87291b00e3b7b7fa64b126d05e2229c42483a06b46ad`. Commit `026f910`
recorded that binding, and every input digest in it equals the file content committed at
`319de72`. The record was therefore internally consistent at that point.

Eight later commits changed input digests or the command list while leaving the source commit
and the package digest untouched:

| Commit | Change to the binding | Package rebuilt |
| --- | --- | --- |
| `f463f63` | Refreshed the three implementation digests | No |
| `79050fd` | Refreshed the campaign implementation digest | No |
| `3c2ac78` | Refreshed the README digest | No |
| `c65bf3b` | Refreshed the README digest | No |
| `ecf6faa` | Refreshed the three implementation digests | No |
| `6180d63` | Refreshed the README digest | No |
| `8238383` | Refreshed a runtime implementation digest | No |
| `26fb3ff` | Added `--review-record` to the package command, refreshed `package_release.py`, README and SECURITY digests | No |

At `7bb3dbf` six of the eleven recorded digests (`team.rs`, `team_runtime.rs`,
`team_integration.rs`, `package_release.py`, `README.md`, `SECURITY.md`) did not match the
content committed at `319de72`, and the command list named a packaging flag that did not exist
when the package was built. The test in `tests/test_multi_agent_matrix.py` compared digests with
the working tree only, so a digest copied from a later tree satisfied it. The first commits that
did not copy digests (`2b34029` for the README and `37e7e51` for `team.rs`) exposed the drift,
which is the failure recorded on 2026-09-20 and retained in `cm-r01/freshness-failure-7bb3dbf.log`.

That failure is not a defect in the freshness test. The defect is that refreshed digests could
pass the test without any re-execution, so the binding stopped describing the evidence it claims.

## CM-R01.3 - Test repair

`binding_errors` now checks each recorded digest against the content committed at the bound
source commit through `git show` and reports `input-provenance:<path>` separately from
`input-drift:<path>`. Three tests cover the repair:

- `test_multi_agent_evidence_binding_matches_its_bound_commit` must pass on every commit.
- `test_evidence_binding_rejects_refreshed_digest_without_rebinding` reproduces the historical
  defect with an injected resolver: the working tree matches the digest but the bound commit does
  not, and the binding is rejected.
- `test_evidence_binding_rejects_missing_bound_commit_content` rejects a bound commit that lacks
  the recorded input.

`test_multi_agent_evidence_binding_is_current` keeps failing on purpose until the renewal in
CM-R01.6 happens. Its message names Sub-task 25.2.4.6 and the drifted paths.

## CM-R01.4 - Provenance restoration and retained receipts

The binding was restored to the `026f910` record so that every digest again equals the content
committed at `319de72` and the command list matches what actually ran for that package. The
package digest and source commit did not change; no new evidence is claimed. The drifted record
at `7bb3dbf` and its failing test output are retained under `docs/evidence/cm-r01/`.

The new capability roadmap named the runtime by a private product name that the tracked-content
privacy test forbids. The four mentions were replaced with the public role name used elsewhere
in this repository (Muse, the standalone execution engine). This corrects the document with
provenance; it does not change any licensing statement.

## CM-R01.5 - Receipt for the bound commands that need no external authority

The receipt was executed inside the implementation sandbox on the working tree that became the
commit carrying this document (the tree includes the `2be3128` repair and the Sprint 32 mission
work). The earlier attempt on `2be3128` alone never obtained the shared build reservation before
the session was restarted, so it produced no receipt. Commands ran with one build job and one
test thread through the shared reservation script; the log is retained privately as
`full-gates-8.log`.

| Command | Exit | Result |
| --- | --- | --- |
| `cargo fmt --all -- --check` | 0 | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | no findings |
| `cargo test --workspace --all-targets --no-fail-fast` | 101 | 463 passed, 3 failed, 2 ignored (guarded sustained qualifications) across 50 test binaries; the 3 failures are the sandbox-environment cases below |
| `python3 scripts/check_architecture.py` | 0 | pass |
| `python3 scripts/docs_check.py` | 0 | pass |
| `python3 -m unittest discover -s tests -p 'test_*.py'` | 1 | 41 tests, 40 pass, 1 designed failure (`test_multi_agent_evidence_binding_is_current`, CM-R01.6) |
| `git diff --check` | 0 | clean |

After the batch completed, only the CM-R01.5 checkbox text in `TASKS.md` and this paragraph
changed before the commit; no Rust source, schema or fixture changed after the receipt.

Findings from executing the workspace suite in this sandbox, which the earlier native UI work had
not run for untouched crates:

- `codingmage-plan::tests::canonical_repository_plan_parses_and_selects_the_first_open_unit`
  failed with `codingmage.plan.missing_goal` because the Sprint 32 through 35 sections added on
  2026-09-21 carried no `**Sprint goal:**` line, so this repository's own task source stopped
  parsing. Fixed forward by adding the four goal lines; the selected first open unit remains
  `16.2.1.2`.
- Six `codingmage-git` fixture tests depended on an ambient Git user identity for later fixture
  commits (`review`, `inventory`, `worktree` and `commit` tests). The fixture runner now sets a
  deterministic author and committer; no product code changed.
- Environment-only failures retained, not repaired: `codingmage-process` tests
  `cancellation_reaps_children_and_grandchildren` and `timeout_and_cancellation_reap_descendants`
  time out waiting for the fixture's grandchild pid file because the request's
  `max_processes = 4` becomes `RLIMIT_NPROC` for a user id that already owns the sandbox's
  launcher, shell and build processes, so the fixture cannot fork; and
  `codingmage-service::tests::native_systemd_analyze_accepts_rendered_user_unit` fails because
  `systemd-analyze --user verify` cannot initialize a user manager here
  (`Failed to lookup RuntimeDirectory path`). Both must be re-run on the operator's desktop host;
  see the human-only register.

## Items that need a person

- Re-run the three environment-only failing tests on the operator's Fedora host with a user
  systemd instance and an uncontended user id, and record the result with the commit hash.
- CM-R01.6 below.

## CM-R01.6 - Package renewal blocker

`scripts/package_release.py` refuses to build unless an external review record binds the exact
`HEAD` with disposition `approved_for_candidate_construction`. That record is an independent
human review (External 4 and the Sprint 26 human-review gate). The implementation worker cannot
author it, so the package, provenance and binding cannot be renewed here. Sub-task 25.2.4.6,
AC 25.4 and Gates 25.3/25.4 stay open with this exact blocker. Editing digests to obtain a passing
test is prohibited by Execution Rule 23.
