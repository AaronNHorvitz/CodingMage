# Sprint 10 Local Evidence

- **Status:** Partially complete; multi-unit progression and crash recovery remain open
- **Source commits:** `10f0e91b0b1cf87c9ff38ab98865e0862b40aab1` and
  `53c85f1331123a1898d8b53174a5f9858d2702ac`
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
- Canonical progression reparses exact before/after plan bytes, accepts exactly one named
  open-to-checked transition backed by completion evidence, and rejects changed titles,
  dependencies, hierarchy, anchors, or any unrelated checkbox drift. It then selects the next
  dependency-ready unblocked unit or returns a truthful no-ready-work result.

## Open Conditions

- Crash injection at every transition and durable replay protection depend on Sprint 12's journal
  and snapshot implementation. Acceptance criteria and Sprint 10 gates remain open until those
  campaigns pass.
- The current vertical slice uses deterministic fake ports. Live Git, Claude, Codex, and gate-port
  composition remains blocked by their open live authority and credential conditions.
