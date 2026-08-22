# Sprint 22 Controlled Campaign Preflight Evidence

- **Status:** Deterministic preparation implemented; real-target report review and campaign remain
  open
- **Implementation commit:** `061a604`
- **Target-artifact boundary correction:** `35ce2b6`
- **Executed:** 2026-08-22 on Fedora Linux with Rust 1.95.0

## Implemented Boundary

`codingmage campaign-preflight` validates one controlled-target authority without creating a
campaign worktree or invoking model inference. It requires an independent ordinary authorization
file whose exact SHA-256 digest is bound by the campaign authority. It then proves a clean dedicated
non-protected branch, exact starting commit, repository identity, canonical task digest, at least ten
open sub-tasks, one pod, exactly ten accepted outcomes, local-only publication, a protected default
branch, and denied external capabilities. The authorization record must be outside the target tree.

The command hashes the exact provider executable bytes and operator-selected model profiles. It
probes only version and help capability surfaces through the guarded process runtime, using the
existing-login boundary. It also binds deterministic gate commands and tiers, gate executable bytes,
the process guard, four operator controls, allowed and denied path projections, and available private
state and scratch storage.

The successful report contains identities, hashes, counts, booleans, and closed role and authority
codes. The binary fixture asserts that it contains no target or executable path, task prose,
authorization prose, model name, provider output, or credential. It also verifies no inference
marker, campaign state, target mutation, or additional worktree appears.

## Guarded Launch

`scripts/qualify_campaign.py` now has two separate invocations. The first runs `doctor` and
`campaign-preflight`, writes the report with mode `0600`, prints its digest, and exits without
running `campaign`. The second recomputes the report and proceeds only when the operator supplies
both its exact SHA-256 digest and `CODINGMAGE_LIVE_QUALIFICATION=I_ACKNOWLEDGE_EXTERNAL_EFFECTS`.
Changed report bytes, a missing digest, a mismatched digest, or a missing acknowledgment fail before
the campaign subprocess. The wrapper also refuses to create the report inside the target repository.

## Passing Verification

```text
python3 -m unittest tests.test_live_qualification -v
```

Result: eight passed, zero failed. The tests prove report creation and permissions, the first-run
stop, exact-digest approval, acknowledgment enforcement, subprocess ordering, and refusal of a
report destination inside the target tree.

```text
cargo test -p codingmage-cli --test campaign_preflight --locked --offline -- --nocapture
```

Result: one passed, zero failed in 14.40 seconds. Authorization-byte, dirty-repository,
default-branch, and incorrect-ceiling mutations all fail before inference.

```text
cargo test -p codingmage-cli --test prescribed_campaign --locked --offline -- --nocapture
```

Result: one passed, zero failed in 169.91 seconds. The existing production disposable schedule
remains intact.

Strict Clippy for the runtime and CLI all targets, workspace formatting, `git diff --check`, and both
case-insensitive downstream-name contamination scans passed. `python3 scripts/docs_check.py` and
the ten documentation and architecture policy unit tests also passed.

## Pre-existing Test Discrepancy

The broader CLI/runtime test command reached one unrelated existing workflow failure:
`serial_campaign_advances_two_reviewed_tasks_without_touching_active_checkout` records three
accepted outcomes while its assertion expects two. The exact isolated test fails identically in an
untouched temporary worktree at baseline commit `b727b75`. No preflight diff changes integration,
checkpoint, or accepted-outcome progression. This discrepancy remains separate work and is not
presented as a passing gate here.

The standalone `python3 scripts/check_architecture.py` command reports the unchanged test-only
`codingmage-cli -> codingmage-soak` development dependency. That edge is present at baseline commit
`b727b75`; this preflight change adds no crate dependency in that direction. The standalone
architecture command is therefore also not presented as passing evidence.

The complete Python discovery suite also rejects the historical multi-agent evidence binding because
this change updates its bound `Cargo.lock` and `README.md` inputs. The binding is intentionally left
stale until its complete package, platform, architecture, and multi-agent evidence command set is
rerun. No historical package digest or source-commit claim was rewritten for this bounded unit.

## Open Boundary

No real controlled target has been selected by the operator. Sub-task 22.3.2.4 remains open until an
exact target, branch, commit, task source, authorization record, provider profiles, paths, and gates
are selected; the generated source-free report is manually inspected; and its digest is approved.
Task 22.3.3, human reconciliation, external publication, parallel expansion, and Sprint 22 Gate 22.3
also remain open.
