# Rust Engineering Campaign and Native Client Delivery

## Accepted Direction

Owner authorization: 2026-09-24, to reconcile the complete product roadmap, implement the
owned Rust capabilities and resume the assigned worker with noncompeting local GPU use.
[Decision](docs/decisions/0017-rust-capability-roadmap-and-staged-delivery.md) records this sequencing amendment.
[Capability catalogue](CAPABILITY-ROADMAP.md) defines CAP-01 through CAP-48.
[TASKS.md](TASKS.md) remains the sole executable work/evidence ledger.

Reconcile the existing native UI and evidence-freshness gaps first, then implement Sprints 32-35 in their dependency order and the owned capability crosswalk below. Preserve Sprint 36 human/live acceptance; unavailable credentials or desktop access do not block independent contract work.

This amendment supersedes older planning-only/worker-paused and narrower assignment text
only for the newly authorized implementation scope. It does not waive security, legal,
independent review, owner acceptance, existing release gates, resource or publication limits.
Completed work and stable task IDs remain intact. No current checkbox changes here.

## Reconcile Before Implementing

The active UI development clone contains 12 implementation commits beyond the planning checkout. Its headless real-coordinator tests use fake providers; real-desktop, screen-reader, clean-installation and live-provider acceptance remain open. Do not overwrite those commits or relabel tests as live qualification.

Read the actual source, current handoff, checks and exact dirty diff before selecting work.
Map reusable behavior to the new acceptance criteria; do not rebuild a capability merely
because it has a new capability identifier. A stale doc is corrected with provenance, not assumed true.
Resolve unblocked foundational defects first; record a genuine external blocker and continue
another dependency-ready owned unit instead of inventing approval.

## Ownership and Interfaces

Own durable campaigns, task/role scheduling, isolated repository operations, immutable review and serialized integration. Preserve the native Rust client already implemented. Consume the standalone execution engine through an optional versioned adapter; do not implement a second tool/research/model loop.

All new first-party production code is Rust. Existing build/test scripts and external runtimes
are documented boundaries. Use established crates, the existing design and toolkit choices.
Every cross-component contract specifies versions, identities, typed outcomes, idempotency,
cancellation/deadlines, authorization and redacted event schemas. Agree producer and consumer
fixtures before integration; an adapter must work independently of a private product identity.

## Execution Packages

The CM-R identifiers in TASKS.md are planning work packages, not
new sprint gates and not substitutes for existing detailed rows. Each package links CAP IDs,
dependencies and acceptance. Split a large package into bounded subtasks in that same ledger
before coding, preserving its ID and mapping to existing tasks. P0 runs first; P1 follows;
P2 remains accepted later scope and must not displace an unblocked P0 task.

## Resource and Model Qualification

Use only the assigned checkout and process budget. One writer per checkout; no worker may
signal, reconfigure or take over another worker. Heavy checks must honor the operator's shared
build reservation. Do not raise memory/CPU limits after pressure; reduce concurrency.

One supervised owner holds the RTX 4090 inference lease for a complete campaign, including
model startup, tests, shutdown and descendant cleanup. An idle-memory snapshot alone is not
a reservation. First qualify the standalone runtime's Muse Glimmer profile; test gpt-oss-20b
separately if admitted and available; only then hand off to native image/text qualification.
Never leave two model servers or background downloads taking the same reserved resources.
CPU-only lanes and labelled software rendering can proceed without claiming GPU qualification.
Unavailable lease, model, account, hardware or license means a recorded blocker, not bypass.

Record exact publisher/artifact identity from admitted metadata, runtime revision, hash, codec,
context, decoding, thread and GPU settings. User shorthand is not evidence of a model publisher
or license. No new paid service, model download allowance, cloud fallback or hardware purchase
is granted by this roadmap. Preserve existing bounded download budgets cumulatively.

## Definition of Done and Authority

Deliver a real vertical workflow, its focused regression/adversarial/recovery tests, an exact
revision-bound receipt and truthful user-visible states. Then batch cumulative checks/evidence
renewal once. Never change input binding or benchmark targets merely to obtain green output.
Implementation, fake-provider tests, real-model qualification, independent review and release
approval are different states. A worker cannot sign its own independent review.

Routine in-scope engineering choices and local commits are delegated. Preserve the current
repository's specific remote/publication restrictions; this request grants no new force push,
main merge, release, account, spending or private-source disclosure authority. Bounded
hands-off product operation must be enforced by code, not by simulating clicks on approvals.
