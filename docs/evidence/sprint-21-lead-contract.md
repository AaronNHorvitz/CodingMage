# Sprint 21 Lead Disposition Contract Evidence

- **Status:** Closed lead contract complete; disposition lifecycle semantics remain open
- **Implementation commit:** `56cd123d6f65abaa085f57e35b99b72dcc4701aa`
- **Executed:** 2026-08-21 on Fedora Linux with Rust 1.95.0

## Contract

The read-only lead report now selects exactly one closed disposition: `propose`, `blocked`,
`deferred`, or `human_decision_required`. The JSON Schema requires all payload slots and uses four
exclusive branches so a proposal cannot coexist with any nonproposal payload. Rust deserialization
rejects unknown fields and unknown enum values independently of provider-side schema enforcement.

Blocked, deferred, and human-decision payloads use closed reason enums. Every nonproposal payload
repeats the exact campaign identity, campaign head, task-source digest, task identity, and canonical
dependency array. Deferrals additionally select one closed reconsideration trigger; there is no
field for a date, prose condition, command, provider, publication mode, or authority expansion.

The deterministic validator compares every binding field against campaign authority and the
coordinator-supplied dependency-ready set. A valid lead response remains untrusted data: it cannot
check a task, create a worktree, invoke an implementer, mutate Git, or otherwise advance lifecycle
state. Those effects remain coordinator-owned.

## Verification

Focused tests cover all four disposition branches, mixed proposal/blocker rejection, invented reason
rejection, unknown-field rejection, and mutation of campaign, head, source digest, task, and
dependency bindings. Codex adapter tests bind campaign identity before returning a report and verify
that the installed schema exposes exactly the four closed choices. Every binary campaign fixture was
migrated to the new schema and passed.

```text
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

## Open Scope

This evidence closes Task `21.2.1` only. Durable blocker continuation is now covered separately by
`docs/evidence/sprint-21-blocker-continuation.md`. Authenticated blocker clearance, deferral
eligibility, repeated-deferral detection, starvation resistance, human-decision continuation, and
the full Story 21.2 gate remain open. The runtime still stops conservatively on deferred and
human-decision dispositions rather than claiming those unimplemented lifecycle semantics.
