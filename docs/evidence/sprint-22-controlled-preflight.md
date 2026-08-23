# Sprint 22 Controlled Campaign Preflight Evidence

- **Status:** Controlled-target preparation and report review complete; executions so far stopped
  safely with zero accepted outcomes, and the revised gate-storage authority
  awaits a new exact-digest approval
- **Implementation commit:** `061a604`
- **Target-artifact boundary correction:** `35ce2b6`
- **Stable approval-projection correction:** `174c320`
- **Provider-compatible lead-schema correction:** `56a1c19`
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
state and scratch storage. Exact free-byte observations remain internal gate inputs; the report
contains only stable per-root threshold results and the configured threshold so unchanged authority
produces identical approval bytes.

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

The first controlled-target launch attempt correctly refused before inference because schema version
1 included volatile exact free-storage byte observations. Schema version 2 replaces those values with
stable threshold booleans, and the binary fixture requires two consecutive preflight reports to be
byte-for-byte identical. Evidence from the refused version 1 report is invalidated.

After approval of the stable report, the first campaign lead process started but returned
`codingmage.provider.codex.failed` before selecting a task or creating an implementation pod. A
source-free profile diagnostic proved authentication and the configured model worked. A second
diagnostic using the exact team-lead schema reproduced the provider's `invalid_json_schema` refusal:
root-level `oneOf` was not accepted by the structured-output endpoint.

Commit `56a1c19` removes conditional schema unions, makes the three disposition payloads nullable
closed objects, and leaves their exact mutual-exclusion and binding rules in the deterministic Rust
validator. A recursive unit assertion forbids `oneOf` anywhere in the provider schema. The corrected
schema was accepted by the real configured Codex endpoint and produced a structured response. Eight
Codex adapter tests, 89 runtime tests, the repeated binary preflight fixture, and strict Clippy pass.
The failed campaign remains retained with zero accepted outcomes and is not resumed or rewritten.

## Controlled-Target Replacement Results

The owner reviewed and approved replacement report digest
`3aeabb522559163dfa9cf30e631ec0b99c2090846d540c9d6bf51a93febb52c2`. The guarded launcher
recomputed the exact report, revalidated the clean dedicated clone and authority, and began the
one-pod local-only campaign. The corrected lead schema was accepted, the lead selected dependency-
ready sub-task `5.2.1.1`, and the implementation provider produced candidate commit
`381cd23e45049b882164f13122eea80321267aa0` on its isolated pod branch.

The candidate passed its focused kernel-contract tests and strict focused Clippy but failed
`cargo fmt --all -- --check` on two import layouts. The configured gate order had already run an
expensive workspace build inside the task worktree. Live task and build state crossed the 1 GiB
retained-state ceiling before bounded correction could begin, so the coordinator paused with
`codingmage.campaign.limit.retained_state_bytes`, zero completed units, and zero accepted outcomes.
After exact owned-worktree cleanup, retained durable state measured about 30 KiB; the lower terminal
number does not erase the measured live ceiling event. The clean source clone, paused campaign,
candidate branch, journal, checkpoint, and both earlier failed campaign records remain preserved.

A second replacement authority moves compiler artifacts to an explicit private campaign-owned build
cache outside retained journal and worktree state and orders formatting and documentation checks
before workspace compilation. It does not increase the 1 GiB retained-state limit or broaden model,
Git, network, publication, path, or process authority. Its schema-v2 source-free report was generated
twice with identical bytes and digest
`009cf73129cf77677364440c8276bb66590e392a49f49157dd5957dbd0aa948b`. The report is mode `0600`,
the dedicated clone is clean at `835049c9e943dade2f1d523708e84ee7e4a3d5d0` with exactly one
worktree, no campaign state exists, and no inference process is active. This changed gate registry is
not authorized by either prior digest and must receive a new exact-digest approval before launch.

```text
cargo test -p codingmage-cli --test campaign_preflight --locked --offline -- --nocapture
```

Result: one passed, zero failed in 28.57 seconds. The fixture includes two byte-identical successful
preflights. Authorization-byte, dirty-repository, default-branch, and incorrect-ceiling mutations all
fail before inference.

```text
cargo test -p codingmage-cli --test prescribed_campaign --locked --offline -- --nocapture
```

Result: one passed, zero failed in 169.91 seconds. The existing production disposable schedule
remains intact.

Strict Clippy for the runtime and CLI all targets, workspace formatting, `git diff --check`, and both
case-insensitive downstream-name contamination scans passed. `python3 scripts/docs_check.py` and
the ten documentation and architecture policy unit tests also passed.

## Reconciled Baseline Findings

Commit `3f2759c` corrected the serial workflow fixture to count its satisfied deferral plus two
completed tasks as three accepted outcomes, matching the integrity-bound campaign contract. The
exact recovery workflow then passed in 76.90 seconds, and the complete workspace run passed with
the production prescribed campaign completing in 167.14 seconds.

Commit `51fe924` granted the existing `codingmage-cli -> codingmage-soak` edge only as an explicit
development dependency. The production graph remains unchanged, and the architecture checker and
mutation tests pass. Commit `4967f6d` refreshed the package binding after two clean builds produced
byte-identical unsigned Linux archives. These corrections remove the prior local discrepancies
without changing the controlled-target authority boundary.

## Open Boundary

Controlled-target preparation and manual review are complete. The revised source-free digest still
requires explicit approval before its first provider invocation. Task 22.3.3, ten accepted outcomes
or another exact terminal result, complete human reconciliation, external publication, parallel
expansion, and Sprint 22 Gate 22.3 remain open. Neither failed execution is represented as controlled-
target qualification evidence.
