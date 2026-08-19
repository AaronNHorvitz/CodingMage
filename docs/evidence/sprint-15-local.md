# Sprint 15 Local Evidence

## Scope

This evidence covers the deny-first GitHub authority boundary, token-blind authentication probe, story-owned issue sections, draft pull-request records, automated review comments, exact feature-branch policy, timeout reconciliation, optimistic concurrency, and complete disablement.

## Authority

- Account, host, owner, repository, feature branch, and protected branch are exact validated identities.
- Issue read/write, pull-request read/write, comments, and branch push are independent default-denied capabilities.
- The adapter exposes no merge, release, settings, secrets, Actions administration, force-push, branch-delete, or protected-branch operation.
- Auth probing uses bounded structured output and rejects unknown fields, including any token field. Its command plans never request or print a raw token.
- Push authorization requires the exact configured nonprotected branch and completed local gates.

## Synchronization

- Story issue sections are enclosed by exact story-specific ownership markers. All bytes outside those markers survive updates.
- Remote checkbox edits are overwritten from canonical local task state and never become completion evidence.
- Writes receive SHA-256 idempotency keys derived from operation, expected remote version, and desired body.
- A timeout triggers an exact key lookup. An absent key returns `uncertain`; it never triggers a blind replay.
- One optimistic-concurrency conflict may refetch current content and rebuild the owned section. The fixture proves a concurrent human edit survives.
- Redirect, identity, authentication, and permission changes fail closed and have content-free durable journal categories.
- Draft PR output binds story, base/head branches, commits, test evidence, findings, limitations, and blockers. Automated findings are explicitly labeled as not human approval.
- The API contains no readiness, approval, or merge transition.

## Verification

Implementation commits: `166ff02` and `a8af1de`.

```text
cargo test -p codingmage-github --all-targets
8 passed; 0 failed

cargo test -p codingmage-state --all-targets
12 passed; 0 failed

cargo clippy -p codingmage-state -p codingmage-github --all-targets -- -D warnings
passed

python3 scripts/check_architecture.py
passed
```

The fake transport covers applied and lost timeout responses, duplicate delivery, version conflict with concurrent human text, redirect, permission reduction, disabled permissions, exact push authorization, draft-only publication, local-authoritative issue state, and durable boundary-change recording.

## Open Evidence

`Gate 15.1` remains open because no authenticated disposable GitHub test repository was changed during this run. The fake-server half passes, but live issue, comment, branch-push, and draft-PR behavior must be exercised against an explicitly authorized disposable repository before the combined gate can close.
