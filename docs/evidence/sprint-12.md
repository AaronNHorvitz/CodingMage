# Sprint 12 Evidence

## Scope

This evidence covers the durable journal, current-state snapshot, exact writer ownership, restart reconciliation, and the Sprint 10 interruption gate unlocked by durable state. The implementation is in `codingmage-state`; `DurableWorkflowPort` composes it with the existing one-unit coordinator.

## Implementation Boundaries

- Every record contains a contiguous sequence, schema version, prior hash, trusted timestamp, typed run/task/repository identities, typed event and outcome, and immutable evidence references.
- SHA-256 covers canonical record fields. Loading validates record size, schema, sequence, prior hash, record hash, and strict unknown-field rejection before exposing state.
- Durable fields are content-minimized. Redaction markers retain only canonical categories; the API has no field for source copies, prompts, credentials, or raw provider transcripts.
- Append uses one JSON record per line and synchronizes the file and state directory before success.
- A nonblocking OS file lock enforces one writer. Normal destruction explicitly unlocks; a hard-killed holder releases ownership through kernel process cleanup.
- Snapshots are derived only from accepted records, written to a temporary file, synchronized, atomically renamed, and checked against both their own hash and the exact journal tip.
- `DurableWorkflowPort` persists an uncertain intent before every delegated operation and a successful observation afterward. State-changing operations interrupted between those records require re-observation; no recovery path replays them.
- Exact repository, worktree, branch, commit, process-start, provider-session, gate, and evidence mismatches block recovery.

## Verification

Implementation commits: `6485d90`, `0108394`, `760a79a`, and `22ef7b5`.

Focused campaigns:

```text
cargo test -p codingmage-state --all-targets
12 passed; 0 failed

cargo test -p codingmage-orchestrator --all-targets
9 passed; 0 failed

for i in 1 2 3 4 5; do cargo test -p codingmage-state --all-targets --quiet; done
five consecutive passes

cargo clippy --workspace --all-targets -- -D warnings
passed

python3 scripts/check_architecture.py
passed
```

The mutation corpus changes every persisted record leaf to a different type-valid value and adds an unknown critical field. Separate cases cover torn JSON, duplicate and reordered records, a broken prior hash, oversized records, mutated snapshot hash/projection/position, and an abandoned temporary snapshot. The orchestration campaign interrupts all ten authority-port operations immediately after durable intent; state-changing work resolves to `reobserve`, while read-only or idempotent work alone may resume.

## Limitations

This sprint proves local durable-state behavior with fake authority ports. Authenticated live provider and network Git executions remain governed by their later integration and external-evidence gates. No such external execution is claimed here.
