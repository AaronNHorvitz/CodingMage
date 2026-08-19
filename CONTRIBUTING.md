# Contributing to CodingMage

CodingMage is in a private bootstrap phase and is not accepting external contributions yet.
This policy records the requirements that apply to owner-authored and agent-assisted changes.
A code of conduct will be added before external contributions are opened.

## Change Workflow

1. Start from a clean, current default branch.
2. Use a focused feature branch and an isolated worktree for automated writes.
3. Keep each commit bounded to one coherent implementation or evidence unit.
4. Run focused tests, repository-wide applicable gates, and `git diff --check`.
5. Review the exact committed diff; do not review an uncommitted approximation.
6. Push only the authorized feature branch. Do not force-push or rewrite published history.
7. Leave merge, release, and external-infrastructure decisions to the repository owner.

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
