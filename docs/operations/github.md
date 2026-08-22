# GitHub

## Authority Boundary

GitHub synchronization is deny-first and optional. Account, host, repository, remote, task branch,
campaign branch, destination branch, commits, issue, pull request, and required checks are exact
identities. Network, branch push, issue write, and draft-PR write are independent grants. Models do
not receive the authenticated CLI or any merge authority.

## Task Workflow

When per-task publication is enabled, the coordinator:

1. Reconciles or creates the assigned task issue immediately after deterministic admission and
   before implementation begins.
2. Preserves one durable mapping across campaign, task, pod, worktree, branch, issue, candidate,
   review sessions, PR, CI, and integration.
3. Pushes only the exact reviewed task branch.
4. Creates or updates one draft task PR targeting the owned campaign branch.
5. Reads required CI only for the exact PR and reviewed commit.
6. Returns attributable CI failure to the same Claude lineage, then requires fresh local gates and
   fresh cumulative Codex review.
7. Updates the existing issue and PR rather than creating duplicates.

CodingMage owns only marker-bounded issue and PR sections. Human bytes outside those markers are
preserved. Automated review is labeled as automated evidence and never impersonates human approval.
Remote task completion follows the canonical integrated task state; GitHub checkboxes never override
the local plan.

## Uncertain Writes

Every external write carries a content-derived idempotency key and expected remote version. A
timeout triggers read-only reconciliation by exact key before retry. Redirect, identity drift,
permission loss, unexpected base or head, changed reviewed SHA, duplicate object, ambiguous CI, or
concurrent mutation outside the owned-section contract fails closed.

The production adapter uses the operator-selected absolute `gh` executable and fixed argument
vectors. No operation exists for force-push, history rewrite, branch deletion, settings, secrets,
Actions administration, release publication, or unscoped repository mutation.

## Integration And Promotion

Task PR eligibility feeds the coordinator's serialized integration queue; model review does not
merge. The safe default permits verified task integration only into the isolated campaign branch
and requires an exact human decision for destination promotion. A final draft PR may be maintained
after final gates and review without promoting it.

Fake transport, command-rendering, idempotency, CI-correction, human-content, and promotion-policy
tests pass locally. Authenticated disposable-repository evidence remains open and must use the
guarded qualification runner documented in [`Quickstart`](quickstart.md).
