# Recovery

CodingMage uses an append-only hash-chained JSON Lines journal and an atomically replaced snapshot. State-changing intent is durable before an effect begins, and a separate observation follows success.

On restart, repository, worktree, branch, commit, process start, provider session, model, gate, and evidence identities are compared. A mismatch blocks recovery. Uncertain state-changing effects require re-observation and are never replayed blindly. Corrupt, replayed, reordered, truncated, stale, or forged records cannot authorize new work.

Operator recovery should preserve the journal, stop the exact owned process group, run diagnosis, and inspect the content-free recovery reason. Deleting state to bypass a refusal destroys evidence and is not a supported recovery method.
