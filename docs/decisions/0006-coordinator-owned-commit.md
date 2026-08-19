# Decision 0006: Coordinator-Owned Commit Boundary

- **Status:** Accepted
- **Date:** 2026-08-19
- **Supersedes:** The provider-commit wording in the initial README and Sprint 5 task text

## Context

Claude must edit an isolated implementation worktree without receiving repository authority. A
linked Git worktree stores branch and index metadata in the target repository's Git directory, so
allowing the provider to run `git commit` would grant writes outside the assigned filesystem tree.
It would also make model-generated shell text an authority path for hooks, signing, helpers, and
other ambient Git behavior.

## Decision

Claude is a file-editing implementation agent. It runs in bare mode with a deny-first profile that
exposes only worktree-scoped `Read`, `Edit`, and `Write`; explicitly denies Git metadata, Bash, web,
subagent, skill, notebook, MCP, credential, and external-infrastructure surfaces; and disables the
unsandboxed-command fallback.

After Claude reports a candidate ready, CodingMage independently inventories all changed paths,
rejects anything outside packet ownership, executes configured deterministic gates, and creates a
commit through a private literal Git operation. The operation uses fixed identity and message
construction, disables hooks and signing, revalidates exact ownership and parent identity before
mutation, and verifies the resulting direct parent and clean worktree.

## Alternatives

- **Let Claude commit directly:** rejected because linked-worktree Git metadata is outside the
  provider's file authority and provider-composed Git commands are not deterministic authority.
- **Give Claude a general Bash allowlist:** rejected because shell grammar and command chaining make
  prefix-pattern policy unsuitable as the sole destructive-operation boundary.
- **Use a standalone clone per attempt:** viable later, but duplicates object storage and does not
  remove the need for coordinator verification and controlled publication.

## Consequences

- Provider test claims remain untrusted; CodingMage runs every authoritative gate itself.
- Provider sessions can prepare edits without Git, network, credential, or publication access.
- Commit creation is an explicit coordinator effect that must be journaled and reconciled before
  review or publication.
- Authenticated live-provider containment remains a separate acceptance gate and is not inferred
  from unit tests or provider documentation.
