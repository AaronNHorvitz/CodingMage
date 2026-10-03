# ADR 0088: Bind Director Planning Proposals to Exact Sources

- **Status:** Accepted for local implementation; bounded review and correction re-review PASS
- **Date:** 2026-10-02
- **Decision owners:** CodingMage implementation

## Context

Sprint 33 starts with a director role that may suggest priorities for approved
work. The existing campaign authority, owner mission charter, parsed task
source and durable pod scheduler already identify the allowed work and the
current planning generation. A new role must not become another execution or
acceptance authority. The native Setup command-equivalence and state-catalogue
work remains open independently.

## Decision

Define version-one `DirectorInput` and `DirectorProposal` packets in
`codingmage-campaign`. A fresh input binds the campaign and repository IDs,
campaign authority, mission charter, task source, complete scheduler digest,
immutable planning generation and current campaign head. It carries exact
owner criteria, source-checkbox sprint counts clearly named as source state,
the complete scheduler-ready set, bounded content-free failure codes and
remaining coordinator limits. Ready follow-up tasks must have existing sealed
scheduler bindings. The caller supplies failure observations and remaining
counters from coordinator state; creating the packet cannot change them.

A proposal can reorder only the complete observed ready set and can refer only
to existing owner-criterion indices. It contains no new task, criterion,
command, path, provider, approval or free-text instruction. The complete
input digest binds its suggestion. Runtime persistence revalidates the input against
fresh authority/source/scheduler/head and writes an integrity-bound private
document named by the proposal digest. A later load revalidates both the
document and the current source. No provider is invoked in this increment;
policy validation of proposed priorities and delegated domains is the next
Task 33.1.1.2 boundary. A persisted proposal is not admitted work.

Reuse `codingmage-state::IntegrityDocument` in `codingmage-runtime` for
private atomic persistence. Architecture policy forbids a campaign-contract
to state-storage dependency, so packets stay in `codingmage-campaign` and the
store stays in the existing runtime owner, which already depends on both.
This adds no dependency or third-party package. It does not change the
coordinator's journal schema or task status.

## Verification

See `docs/evidence/sprint-33-director-packets.md` for exact source,
round-trip, stale-head, malformed, tamper and cumulative check results.
Runtime role qualification, human acceptance and independent review remain
separate gates.
The first bounded exact-commit review returned PASS with a Medium finding on
the failure-code vocabulary. Decision 0089 closes it; fresh independent
read-only re-review of exact commit
`2753c969c725408e470122da4b7b123c2f217969` returned PASS with no
findings and explicitly closed that Medium. This remains bounded review,
not human acceptance or runtime qualification.
