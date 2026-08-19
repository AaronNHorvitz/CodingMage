# ADR 0002: External Target and No Self-Modification

- **Status:** Accepted
- **Date:** 2026-08-19
- **Decision owners:** Repository owner
- **Supersedes:** None
- **Superseded by:** None

## Context

A coordinator that can rewrite its own policy or executable while running can erase the boundary
that authorizes it. Writing into a user's active checkout also risks destroying unrelated work.

## Decision

CodingMage operates from outside every target repository. It must reject its own source, runtime
state, or overlapping roots as targets. Automated writes occur only in a uniquely owned target
worktree after repository identity is authorized and revalidated. Running instances never modify,
replace, merge, or update CodingMage itself.

## Alternatives Considered

- Self-hosting CodingMage against its own repository would provide a convenient test target but
  would let untrusted target content influence the coordinator's authority boundary.
- Editing the active checkout would reduce worktree overhead but would endanger user changes.
- Allowing self-update with review was rejected because a running process cannot independently
  validate the authority that replaces it.

## Consequences

Development of CodingMage uses ordinary human-controlled workflows. Integration tests use fixture
repositories or separately authorized targets. Updates require an explicit operator action after
review, and worktree identity becomes a load-bearing contract.

## Verification

- Overlap, symlink, renamed-directory, and self-target fixtures fail closed.
- Tests prove the active checkout and unrelated worktrees are preserved byte-for-byte.
- No runtime API grants self-update or merge authority.
