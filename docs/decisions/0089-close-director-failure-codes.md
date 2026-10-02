# ADR 0089: Close Director Failure Codes Over Campaign Task Reasons

- **Status:** Accepted for local correction; exact-commit re-review pending
- **Date:** 2026-10-02
- **Decision owners:** CodingMage implementation

## Context

The bounded independent review of `d68a25fe4faf971261cbcde632506215bdb4b380`
returned PASS with a Medium finding: `DirectorFailure.code` accepted arbitrary
snake-case strings. A caller could place content in a syntactically valid code,
contradicting Decision 0088's content-free packet contract. No director provider
is invoked by that increment, but the packet must already reject such input.

## Decision

Represent a director failure code as the closed `DirectorFailureCode` enum. Its
variants are the non-success `TaskTerminalReason` cases in the existing
campaign task lifecycle. `TryFrom<TaskTerminalReason>` is exhaustive and
refuses `Merged`; serialized packets accept only those named variants. The
runtime producer, when added, must derive observations from coordinator task
state and its typed terminal reason. A provider or repository cannot supply a
new code string. The existing task identity, source and packet checks remain.

This correction changes neither scheduler admission nor task status. It adds
no provider invocation, dependency, credential, model or third-party material.

## Verification

See `docs/evidence/sprint-33-director-code-correction.md`. The focused test
round-trips every admitted typed reason and rejects an unknown valid-looking
string, free-form prose and the success reason. Independent re-review remains
required for this correction; human and live-provider gates remain open.
