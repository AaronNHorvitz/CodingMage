# Sprint 17 Unattended Pilot Handoff

- **Report date:** 2026-08-21
- **Repository:** CodingMage only
- **Branch:** `feat/hierarchical-campaigns`
- **Evidence baseline:** `b5c08ca0c34939be109f9094b9d2d88f43b4ba30`
- **Publication authority:** local only
- **Human acceptance:** pending

## Executive Status

The bounded unattended pilot is complete at the implementation, correction, blocker, and quota
boundaries authorized for Sprint 17. One disposable story completed through implementation, senior
review, one correction, integration, task completion, and durable checkpoint without operator
intervention. Nine additional disposable campaigns stopped exactly on all six external-blocker
reasons valid for dependency-ready work and on quota at each provider boundary.

No pilot pushed or merged a fixture branch, opened or changed an issue or pull request, published a
release, used external infrastructure, or changed an active checkout. Campaign-root worktrees and
content-minimized private state remain intentionally available for reobservation. Implementation
pod worktrees and owned processes were released.

## Completed Story Status

| Field | Reconciled value |
|---|---|
| state | `complete` |
| stop reason | `completion` |
| task | `0.1.1.1` |
| completed units | 1 |
| provider attempts | 5 |
| correction rounds | 1 |
| malformed-report repairs | 0 |
| process invocations | 16 |
| observed output bytes | 15,209 |
| retained state bytes | 42,437 |
| base commit | `ff39399595a1fd024d37a44832339a8ce34f90de` |
| reconciled campaign head | `8b1e986070677e7792183ca458bd9e81d764eca6` |

The active checkout retained its base commit and clean status. The campaign head contains the exact
review-corrected artifact and the mechanically completed canonical task marker. The private
checkpoint has no active unit, pending integration, blocker, or unreconciled correction.

## Blocker And Quota Status

| Case family | Cases | Terminal classification | Completed units |
|---|---:|---|---:|
| valid dependency-ready external blockers | 6 | `blocked` / `no_independent_ready_work` | 0 |
| team-lead quota | 1 | `paused` / `capacity_pause` | 0 |
| implementer quota | 1 | `paused` / `capacity_pause` | 0 |
| reviewer quota | 1 | `paused` / `capacity_pause` | 0 |

Every blocker campaign invoked only the lead once. The quota campaigns stopped after exactly
`lead`; `lead, implementer`; and `lead, implementer, reviewer`, respectively. No case retried,
completed a task, or changed its active checkout. All nine durable statuses reconcile with the
terminal result, all nine campaign task sources remain unchecked, each fixture retains exactly one
campaign-root worktree, all pod-scratch roots are empty, and no qualification-owned process remains.

The retained nine-case fixture set occupies 554,907 bytes. Its content-minimized summary SHA-256 is
`d0c9237f57360e5357559d954ba7bbe3b4876e504cb9ce718ce559ace73301ad`.

## Build And Evidence Identity

| Artifact | Identity |
|---|---|
| one-story harness correction | `03d4674` |
| one-story evidence reconciliation | `0ff1ad3` |
| blocker/quota harness | `593c53c` |
| blocker terminal correction | `a31328b` |
| paused-task identity correction | `79c671f` |
| blocker/quota evidence reconciliation | `b5c08ca` |
| release CodingMage binary SHA-256 | `a4d774eaf858d37043b29f15e42c880c8e7e2aeffaec790ba557d97033bc9425` |
| blocker/quota pilot SHA-256 | `45195a3e56abdc6e29819515a0693c419632fa4c6fe09df73dd05e8e02105134` |

The rejected prequalification attempts are retained as failed harness evidence. One applied the
unit ceiling before the blocker terminal projection could be observed; the other expected a null
last-task identity where the coordinator correctly retained the attempted task for resume. Neither
attempt changed production code or created a false passing claim.

## Reinspection

From a clean CodingMage checkout, a human reviewer can verify repository evidence with:

```bash
git status --short --branch
git log --oneline --decorate -10
python3 scripts/docs_check.py --root .
python3 scripts/check_architecture.py --root .
cargo test -p codingmage-soak --all-targets
cargo clippy -p codingmage-soak --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git diff --check
```

Retained private fixtures live below the operator's CodingMage qualification-state root and are not
part of the repository. Their `campaign-status` projections can be read without mutation using the
commands documented in `docs/operations/serial-campaign.md`. Repeated status reads must not change
the checkpoint, consume a provider attempt, start a process, or alter repository state.

## Open Limits

- Human comparison of this report, retained state, repository history, and source claims remains
  open; this report does not self-approve `AC 17.2`.
- The production coordinator has not yet run the complete prescribed ten-outcome schedule required
  by Story 22.3.
- A separately authorized ten-task controlled-target campaign has not run.
- Multi-pod execution remains disabled until the one-pod production qualification gate passes.
- Authenticated network Git and GitHub mutation evidence remains external.
- Native macOS and Windows evidence remains external.
- Manual fuzzing and independent human security and architecture review remain deferred release
  gates.
- No merge, tag, package publication, release publication, or public-release approval is claimed.

## Exact Continuation

The next locally implementable unit is Sprint 22 sub-task `22.3.1.1`: run the production campaign
coordinator against the prescribed ten-outcome disposable schedule. Preserve one-pod, local-only
authority and keep every broader claim and external gate open until its own evidence exists.
