# Sprint 6 Review Scope Evidence

## Scope

This evidence completes the local Codex senior-review boundary. It covers exact base/target validation, read-only invocation plans, thread identity, structured findings, file and line verification, and before/after scope revalidation. It does not claim a paid or quota-consuming live Codex review.

## Review Binding

- Base and target must be full object identifiers, distinct, present, and in an ancestor relationship.
- The review checkout HEAD must equal the target commit.
- Hardened Git inventories the exact target tree with a bounded canonical path set and tree digest.
- Every provider finding with a source location must reference a path present in that tree and a one-based line present in the exact target blob.
- Missing, escaping, non-UTF-8, zero-line, out-of-range, and mismatched file/line pairs fail closed.
- Base, target, HEAD, and tree are recaptured after provider execution and must equal the pre-review scope.

## Authority

Codex receives a read-only sandbox plan, exact worktree, exact commits, exact evidence identities, ignored user configuration/rules, a fixed output schema, and no environment. Its adapter has no repository write, publication, merge, release, task-state, or gate-completion operation. Provider prose remains an untrusted structured report.

## Verification

Implementation commit: `635c94c`.

```text
cargo test -p codingmage-git -p codingmage-codex --all-targets
24 passed; 0 failed

cargo clippy -p codingmage-git -p codingmage-codex --all-targets --all-features -- -D warnings
passed

python3 scripts/check_architecture.py
passed

git diff --check
passed
```

The corpus includes valid exact findings, stale commit claims, duplicate finding identifiers, escaping paths, malformed/changed threads, nonancestor commits, wrong checkout HEAD, missing files, and out-of-range lines.
