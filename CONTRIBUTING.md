# Contributing to CodingMage

CodingMage is in a pre-release bootstrap phase and is not accepting external contributions yet.
This policy records the requirements that apply to owner-authored and agent-assisted changes.
A code of conduct will be added before external contributions are opened.

## Contribution Provenance

Every owner-authored, agent-assisted, and future external contribution must contain only material
the contributor is authorized to submit and license under Apache-2.0. Contributions must not
include employer or client source code, confidential information, credentials, proprietary
materials, nonpublic work product, or third-party content without documented permission and
compatible licensing. AI assistance does not transfer that responsibility away from the person
submitting and reviewing the change.

External contributions will remain closed until the project adopts an explicit contributor
sign-off or equivalent provenance process.

## Change Workflow

1. Start from a clean, current default branch.
2. Use a focused feature branch and an isolated worktree for automated writes.
3. Keep each commit bounded to one coherent implementation or evidence unit.
4. Run focused tests, repository-wide applicable gates, and `git diff --check`.
5. Review the exact committed diff; do not review an uncommitted approximation.
6. Push only the authorized feature branch. Do not force-push or rewrite published history.
7. Leave merge, release, and external-infrastructure authority with the repository owner; perform
   only exact effects already admitted by the applicable policy, never infer them from autonomy.

Changes to unattended execution, provider authority, Git effects, recovery, operator controls,
soak evidence, or release behavior must preserve
[`Unattended Safeguards`](docs/architecture/unattended-safeguards.md) and record a superseding
architecture decision before intentionally changing that contract.

[Decision 0014](docs/decisions/0014-owner-optional-engineering-team.md) accepts planned owner
involvement and bounded engineering delegation. It does not enable those modes or authorize this
repository's development agents to publish, merge, release, redirect other workers or sign their
own independent review. Treat PRD/development-plan changes as requirements, not runtime grants.

## Evidence

A checked task must point to implementation and reproducible tests. Evidence must identify the
source commit, commands, exit status, relevant test counts, limitations, and skipped external
prerequisites. It must not contain credentials, private source excerpts, unrestricted environment
dumps, or hidden model reasoning.

Documentation-only changes must run:

```bash
python3 scripts/docs_check.py
python3 -m unittest discover -s tests -p 'test_*.py'
git diff --check
```

Rust changes must additionally pass formatting, strict Clippy, and all applicable workspace tests.

## Commit Messages

Use an imperative subject that names the behavior, for example:

```text
feat(core): validate repository identity before writes
test(git): cover replaced worktree refusal
docs: record provider adapter boundary
```

Do not commit runtime state, credentials, provider caches, target source copies, generated
worktrees, or machine-specific logs.
