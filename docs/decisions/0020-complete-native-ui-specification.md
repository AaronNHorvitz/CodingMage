# ADR 0020: Complete Native UI Specification and Qualification

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decision owners:** Repository owner
- **Supersedes:** None
- **Superseded by:** None

## Context

CM-UI-001 through CM-UI-006 define the native workspace, and Sprint 36 implemented most of it
against fake providers. The owner asked that CodingMage's UI meet the same specification standard
as the other stack products: a complete screen and state inventory, one interface for every skill
level, an accessibility target, performance budgets on declared devices, and human trials.

## Decision

1. [The native UI specification](../architecture/native-ui-specification.md) is binding. The PRD
   adds CM-UI-007 through CM-UI-015 and TASKS adds Story 36.3. Every new row starts open.
2. There is one interface for everyone, simple by default with depth in context. No beginner,
   expert or advanced mode is introduced. Supervised, exception-only and hands-off remain campaign
   authority settings under CM-TEAM-002, not UI modes.
3. egui with AccessKit (Decision 0015) and the command boundary (Decision 0016) are unchanged.
   Every UI action has a `codingmage` command equivalent that the UI can show.
4. Task 36.3.1 freezes device profiles, numeric budgets and the trial protocol before any
   measurement. Fake-provider runs never count as human trials or live qualification.

## Consequences

Sprint 36 grows by one story. Its existing implementation evidence stays valid for what it
covered; new acceptance is measured against the specification. No gate, human-only row or
release boundary is relaxed. This decision implements nothing.

## Verification

`python3 scripts/docs_check.py` passes and `git diff --check` passes. These show documentation
consistency, not a qualified UI.
