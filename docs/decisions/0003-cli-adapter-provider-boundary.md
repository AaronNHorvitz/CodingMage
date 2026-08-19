# ADR 0003: CLI-Adapter-First Provider Boundary

- **Status:** Accepted
- **Date:** 2026-08-19
- **Decision owners:** Repository owner
- **Supersedes:** None
- **Superseded by:** None

## Context

Claude Code and Codex already expose supported command-line interfaces. CodingMage needs to
coordinate them without importing provider internals, assuming undocumented transcripts, or
granting either provider direct orchestration authority.

## Decision

Integrate initial providers through typed CLI adapters behind a provider-neutral contract. The
coordinator owns work packets, process limits, environment allowlists, state transitions, and
validation. Adapters translate only supported inputs and observable outputs. Provider prose is
untrusted data and cannot complete a task or alter policy by itself.

## Alternatives Considered

- Direct SDK integration could expose richer events but would increase credential handling and
  couple the core to provider-specific APIs.
- Terminal scraping was rejected because presentation text is unstable and ambiguous.
- A shared unrestricted shell wrapper was rejected because it collapses command authority into
  model-generated text.

## Consequences

The first release depends on installed, version-compatible CLIs and must probe their capabilities.
Adapters can be replaced by SDK or local-model implementations later without changing the core
authority contract. Unsupported output or version drift fails closed.

## Verification

- Fake executables cover success, malformed output, timeout, cancellation, and version drift.
- Process requests use executable plus argument arrays, never shell strings.
- Adapter output cannot directly mutate coordinator state.
