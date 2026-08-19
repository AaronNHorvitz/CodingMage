# Sprint 13 Evidence

## Scope

This evidence covers the stable status schema, bounded reconnectable event stream, terminal and JSON rendering, read-only commands, and journal-backed lifecycle controls in `codingmage-monitor`.

## Implemented Controls

Read-only commands are represented by a distinct request path:

- `status`
- `explain-blocker`
- `open-diff`
- `open-log`
- `doctor`

Lifecycle controls require a private mutation grant produced only after exact local-user and run authentication:

- `pause`
- `resume`
- `stop-after-unit`
- `cancel`

The protocol is implemented independently of the Sprint 14 service transport. This keeps a read request structurally unable to acquire lifecycle authority. Each lifecycle request has a canonical idempotency identity and is durably recorded before in-memory state changes. Restart reconstruction consumes only accepted exact-run journal records.

## Privacy and Ordering

- Status contains validated identities, state labels, counts, elapsed time, and known-or-unknown metrics.
- It has no fields for credentials, prompts, hidden reasoning, source text, command arguments, or command output.
- Human and JSON renderings preserve `unknown` instead of converting unavailable usage or reset information to zero.
- Accepted events have contiguous sequences and exact correlation IDs.
- Updates inside the 250 millisecond progress window update the current snapshot but do not create mutable or duplicate sequence entries.
- Reconnecting returns the same authoritative current status and retained immutable events without a coordinator mutation handle.

## Verification

Implementation commit: `aaa4f37`.

```text
cargo test -p codingmage-monitor --all-targets
5 passed; 0 failed

cargo test -p codingmage-state --all-targets
12 passed; 0 failed

cargo clippy -p codingmage-state -p codingmage-monitor --all-targets -- -D warnings
passed

python3 scripts/check_architecture.py
passed
```

Fixtures prove terminal and JSON rendering, unknown metrics, strict unknown-field rejection, bounded coalescing, attach/disconnect/reattach equivalence, all five read commands, all four lifecycle controls, same-user and exact-run rejection, duplicate suppression, and cancellation reconstruction after restart.

## Limitation

Sprint 13 defines and verifies the operator protocol and rendering surface. The unprivileged local service transport that receives these requests is Sprint 14 work and is not claimed by this evidence.
