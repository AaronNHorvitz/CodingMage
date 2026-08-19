# Sprint 10 Local Evidence

- **Status:** Partially complete; multi-unit progression and crash recovery remain open
- **Source commit:** `10f0e91b0b1cf87c9ff38ab98865e0862b40aab1`
- **Executed:** 2026-08-19 on Fedora Linux with Rust 1.95.0
- **Scope:** Legal task lifecycle and one-unit fake-port coordinator

## Commands

The 86-test Rust workspace, formatting, strict Clippy, architecture policy, documentation checks,
and `git diff --check` passed. Five focused coordinator tests exercise legal and hostile event paths.

## Verified Behavior

- The state model includes discovered, ready, claimed, implementing, local verification, senior
  review, correcting, final verification, checkpointed, complete, blocked, paused, recoverable
  failure, terminal failure, and cancelled states.
- Every accepted transition binds the exact run, task, contiguous sequence, required prior state,
  resulting state, unique evidence identities, and coordinator-owned side-effect intent.
- Cross-run, cross-task, duplicate, stale, reordered, skipped, contradictory, evidence-free, and
  evidence-reuse transitions fail without changing state.
- The narrow workflow port composes claim, owned implementation setup, implementation completion,
  deterministic gates, senior review, correction, final verification, checkpoint, canonical
  reconciliation, and exact resource release without giving adapters task-state authority.
- A complete fake vertical slice advances exactly one unit. Separate fixtures cover one bounded
  correction, precise blocking, recoverable and terminal gate failures, and a port failure.
- Once a claim succeeds, release is attempted on every success and failure return path. A release
  failure remains visible rather than being hidden by an earlier result.

## Open Conditions

- Multi-unit progression, canonical plan completion reconciliation, continuing past independent
  blockers, and no-ready-work termination remain open under Story 10.2.
- Crash injection at every transition and durable replay protection depend on Sprint 12's journal
  and snapshot implementation. Acceptance criteria and Sprint 10 gates remain open until those
  campaigns pass.
- The current vertical slice uses deterministic fake ports. Live Git, Claude, Codex, and gate-port
  composition remains blocked by their open live authority and credential conditions.
