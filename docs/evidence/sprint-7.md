# Sprint 7 Evidence

- **Status:** Passed locally
- **Source commit:** `4a862fd20c7617d331353f3281e756d7bc07b813`
- **Executed:** 2026-08-19 on Fedora Linux with Rust 1.95.0
- **Scope:** Strict Markdown task source, dependency selection, work packets, and decomposition

## Commands

The 68-test Rust workspace, formatting, strict Clippy, rustdoc, architecture policy,
documentation checks, eight Python mutation tests, and `git diff --check` passed.

## Verified Behavior

- The parser reads sprint, story, task, sub-task, acceptance, gate, goal, state, and explicit
  dependency records without changing source bytes. Whole-source and exact-line SHA-256 identities
  preserve provenance.
- Namespace-aware duplicate detection supports intentionally corresponding task, acceptance, and
  gate numbers while rejecting duplicates within a namespace. Dependencies must resolve to one
  exact checklist item.
- Missing parents, malformed dotted hierarchy, contradictory checked-parent/open-child state,
  unknown dependencies, duplicate dependencies, invalid UTF-8, excessive input, and stale source
  bytes fail closed.
- Selection returns only the first open, unblocked, dependency-ready sub-task. Explicit blockers
  remain open, and vague or excessive units require decomposition.
- The canonical repository `TASKS.md` is a live test fixture. At the source checkpoint, parsing it
  selected open sub-task 5.2.2.1 without rewriting the file.
- Versioned work packets bind source, repository, commit, branch, worktree, paths, commands,
  acceptance criteria, risks, limits, prohibited actions, artifacts, and blocker namespace. Their
  canonical JSON bodies carry a recomputable hash.
- Decomposition requires at least two uniquely identified children, complete mapping of every
  original acceptance criterion, relative paths, and no ownership expansion without explicit
  material-scope approval.

## Limitations

- The reference grammar is intentionally strict and recognizes CodingMage's documented Markdown
  structure plus explicit `depends-on` comments. Other task sources require adapters in later
  sprints.
- Source checkboxes remain claims from the canonical plan; they do not replace deterministic
  implementation evidence.
