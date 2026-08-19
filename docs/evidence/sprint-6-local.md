# Sprint 6 Local Evidence

- **Status:** Partially complete; live and repository-reconciliation gates remain open
- **Source commit:** `8f8981da668cd616416a3336af36601e88dbc317`
- **Executed:** 2026-08-19 on Fedora Linux with Rust 1.95.0
- **Scope:** Codex capability, thread, read-only invocation, JSONL, and review-report contracts

## Commands

The 62-test Rust workspace, formatting, strict Clippy, rustdoc, architecture policy,
documentation checks, eight Python mutation tests, and `git diff --check` passed. A
content-minimized local probe identified Codex CLI 0.144.5 and the required noninteractive JSONL,
output-schema, exact-thread resume, model, configuration, and read-only sandbox flags without
inspecting authentication state.

## Verified Behavior

- Capability probes pin executable identity, clear the environment, bound output and time, and
  reject unsupported versions or missing behavior.
- Start packets bind the run, task, reviewer, base commit, target commit, evidence identities, and
  exact read-only checkout. Resume requires the exact retained UUID-shaped thread.
- Invocation selects the read-only sandbox, approval policy `never`, strict configuration, ignored
  user rules and settings, literal arguments, bounded standard input and output, and the shared
  process cancellation and descendant cleanup contract.
- The output schema is a repository-owned artifact whose exact bytes are checked both when the
  adapter is built and immediately before execution.
- JSONL parsing requires one exact thread and a structured final response. Unknown events,
  malformed output, changed threads, stale commit claims, duplicate finding identifiers,
  contradictory verdicts, path traversal, and zero source lines fail closed.
- Findings distinguish defects, external blockers, and suggestions. Provider prose receives no
  repository-write, publication, merge, release, or task-state handle.

## Open Conditions

- No authenticated model invocation or known-defect live corpus was executed. Empty-environment
  invocation intentionally avoids copying authentication state but still needs an approved
  credential mediation design for live use.
- The adapter validates report scope structurally. The coordinator must still revalidate base and
  target commits before and after review and prove that every referenced file and line exists in
  the exact target commit.
- Live stale-review, repository-mutation, credential-redaction, and resume campaigns remain open,
  so Story 6.2, both acceptance criteria, and both Sprint 6 gates remain open.
