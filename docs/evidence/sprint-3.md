# Sprint 3 Evidence

- **Status:** Passed on the Linux reference platform
- **Source commit:** `33402665200f81cb57297960c13b7f9a5d64eb11`
- **Executed:** 2026-08-19 on Fedora Linux with Git 2.55.0
- **Scope:** Read-only inventory, prohibited Git policy, and owned worktree lifecycle

## Commands

The 13-test live Git suite passed three consecutive times from the clean source commit. The complete
34-test Rust workspace, formatting, strict Clippy, rustdoc, architecture, documentation,
eight-test Python mutation suite, and `git diff --check` gates then exited successfully.

## Verified Behavior

- Inventory uses private literal Git variants, an empty inherited environment, disabled hooks,
  filesystem monitor, pagers, editors, signers, credential helpers, replacement objects, prompts,
  and user-selected protocols. Output, records, retained bytes, and runtime are bounded.
- Inventory classifies clean, staged, unstaged, untracked, conflicted, detached, merging, rebasing,
  bisecting, and locked states. It retains hashes and counts rather than changed path names.
- Hostile alias, pager, editor, signer, credential-helper, hook, filter, URL-rewrite, alternate-object,
  and attribute fixtures fired no canary and made no connection to a listening loopback socket.
- Worktree creation starts from an exact object, uses a collision-resistant branch and destination,
  writes a private atomic ownership manifest, and revalidates path, physical identity, registration,
  branch, lineage, and cleanliness before removal.
- A dirty active checkout, index, untracked file, notes, stash, tag, configuration, hooks, retained
  owned branch, concurrently changing user file, and similarly named user directory remained exact
  through create and remove. A dropped in-memory owner was recovered from its exact manifest.
- Dirty, renamed, replaced, hostile-checkout, malformed-source, and unowned worktrees fail closed.
  Configured Git requests other than exact `git diff --check` are rejected before spawn, including
  reset, clean, overwrite, delete, prune, garbage collection, rewrite, merge, and push operations.

## Limitations

- The private Git runner terminates its direct Git child on timeout. General descendant containment
  and parent-crash cleanup belong to Sprint 4 and are not claimed here.
- Worktree branch cleanup is deliberately absent; branch deletion is prohibited in the bootstrap
  release. The retained branch is evidence of the completed work rather than abandoned authority.
- Network Git, authenticated publication, merge, release, and cleanup by name pattern remain
  unavailable.
