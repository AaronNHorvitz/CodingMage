# Main-First Development and Publication

Accepted owner instruction: 2026-09-24. The owner requested active consolidation of
accumulated development work into main and an end to unpublished development silos.

## Canonical Branch

main is the current, public-facing development line, not a claim of production readiness.
The designated operator may consolidate existing committed development history into main,
preserving divergent owner changes and accurately recording outstanding verification.
New completed implementation batches are committed and published to main regularly.
A main update does not close independent review, model admission, release or distribution gates.

This instruction supersedes earlier development-branch-only/local-only publication restrictions
for this repository's own development. It does not alter the runtime product's permission
model, its isolated target repositories, or its customers' publication policies.

## Working Rules

- Keep one designated writer per checkout. Preserve unfinished edits; never publish them
  merely to make Git status clean. Commit coherent tested batches, with honest evidence.
- An isolated worker checkout is an execution boundary, not an alternate long-lived product.
  Its completed commits must be delivered to canonical main through the assigned publisher.
- Use the existing committed history. Fast-forward where possible; use a real merge when
  both sides contain changes. Never force-push, reset published history or silently drop work.
- Publish an exact commit and its verification disposition. The publisher checks repository
  identity and fast-forward ancestry and refuses an unexpected or diverged remote.
- Review the exact diff, run applicable checks and retain failures/limitations before
  requesting publication. A model summary alone does not prove a test passed.
- Prefer local main development for the single assigned writer. Use a short-lived branch only
  for genuine concurrent or risky isolation, then integrate it promptly after checks.
- No private consumer identity, credentials, raw private data, model weights, build caches or
  machine-local transcripts belong in a public repository. Preserve existing license terms.
- No release, package deployment, visibility change, new account, purchase, destructive
  cleanup or branch deletion is authorized by this main-first policy.
- A publication conflict is a precise blocker to publication, not permission to overwrite
  the remote or to accumulate unrelated unfinished work without reporting it.

## Current Consolidation

The initial consolidation brings previously committed development snapshots and the accepted
48-capability roadmap into main. In-flight source edits remain local until verified and committed.
Existing historical evidence stays bound to its tested revision; merging history is not fresh
qualification. Keep README, TASKS and verification records explicit about pre-alpha limitations.

Private operator recovery bundles preserve the starting refs and unfinished patches.
Old branches are retained during this pass; deleting historical branches or worktrees requires
a separate inventory and must not discard unique commits or retained evidence.
