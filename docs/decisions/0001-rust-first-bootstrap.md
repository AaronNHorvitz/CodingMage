# ADR 0001: Rust-First Bootstrap Coordinator

- **Status:** Accepted
- **Date:** 2026-08-19
- **Decision owners:** Repository owner
- **Supersedes:** None
- **Superseded by:** None

## Context

CodingMage must enforce typed state transitions, strict authority boundaries, bounded process
execution, durable recovery, and cross-platform packaging. The coordinator is security-sensitive
infrastructure rather than a collection of prompts.

## Decision

Implement the coordinator as a Rust workspace. Keep public identities and wire contracts in a
dependency-light contracts crate, orchestration policy in a core crate, and operator commands in
a CLI crate. Rust code does not begin until Sprint 0 documentation gates pass.

## Alternatives Considered

- Python would shorten the bootstrap but provides weaker compile-time enforcement for state and
  adapter boundaries.
- TypeScript would align with VS Code extension development but would couple the authority core
  to a UI runtime before the headless workflow is proven.
- A shell-script coordinator would make safe argument, process, and recovery boundaries too easy
  to bypass.

## Consequences

The initial build is more deliberate, and dependencies require provenance review. In return,
contracts, errors, and state transitions can be exhaustive and testable. Future UI or service
surfaces must call the Rust authority core instead of reimplementing policy.

## Verification

- Workspace dependency-direction tests reject forbidden edges.
- Strict Clippy and workspace tests pass without warnings.
- Public contracts reject unknown or invalid states.
