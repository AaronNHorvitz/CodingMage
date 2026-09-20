# Decision 0013: Optional Ecosystem Integration Without Authority Transfer

- **Status:** Accepted for planning and staged local implementation
- **Date:** 2026-09-20
- **Decision owners:** Repository owner
- **Supersedes:** None
- **Superseded by:** None

## Context

The owner requested an integration-ready development plan before another coding tool begins work.
The existing provider-neutral contracts, independent coordinator and durable journal are suitable
foundations. They do not yet provide a qualified host application interface, USTE contextual-memory
consumer or Muse Code worker adapter. Development-tool selection and runtime-provider support are
different decisions.

## Decision

1. Keep Decisions 0002 through 0005 in force. CodingMage retains its separate repository, process,
   state, worktrees, credentials, release lifecycle and deterministic enforcement authority.
2. Admit the requirements in [Ecosystem Integration](../architecture/ecosystem-integration.md) and
   Sprints 29 through 31 of [TASKS.md](../../TASKS.md) as planned work. Existing dependency-ready
   first-release work has priority. Optional integrations do not become new first-release gates.
3. Permit dependency-independent local schemas, fakes and consumer tests only under the declared
   task dependencies and fallback scheduling rule. This does not admit work into another product's
   roadmap, grant access to another repository, or enable a live integration.
4. Make the host application a client of an explicitly authorized CodingMage interface. It may
   request work and display controls; it cannot bypass coordinator validation or supply new policy
   through task prose. Human-only actions remain human-only through every client.
5. Treat USTE as an optional context source. The existing journal remains authoritative. Memory
   results cannot grant permissions, attest verification, clear blockers or complete tasks. No
   orchestration-state migration is admitted by this decision.
6. Keep Muse development use outside the running CodingMage product. A future runtime adapter must
   satisfy the existing provider contract and confine every background worker under coordinator
   limits. Unsupported containment or protocol behavior is a blocker, never a reason to bypass it.
7. Require committed version pins, consumer-side compatibility tests and explicit admission on
   both sides before live cross-product integration. Use disposable targets first. A bounded
   qualification supports only its recorded operations, versions and platform.
8. Preserve the downstream-name privacy check. Public documents use the role "host application";
   private target names, paths, credentials and operational mappings remain outside the repository.

The owner's 2026-09-20 request authorizes this planning increment and its branch push. It does not
constitute independent security review, counterpart-project approval, provider provisioning or
release publication authority.

## Alternatives Considered

- Converting CodingMage immediately into a host-owned module would bypass Decision 0005's
  compatibility, migration, shadow, rollback, independent-review and owner-approval conditions.
- Replacing the journal with USTE would change recovery authority before a migration is designed
  or qualified, contrary to Decision 0004.
- Wrapping an unrestricted Muse terminal would bypass Decision 0003's structured provider contract.
- Blocking standalone delivery on all three integrations would add unrelated release dependencies.

## Consequences

The first-release path continues while later interfaces are made explicit. Contract work may use
local fakes when its dependencies are ready; fake results do not establish live compatibility.
Cross-product implementations remain separately owned. Changes to these boundaries require a new
accepted decision naming the affected records.

## Verification

The implementation gates remain unchecked. Required evidence includes request replay and crash
reconciliation, cross-project isolation, authority mutation refusal, unavailable and hostile memory,
provider version drift, nested-worker confinement, cancellation and independently reviewed exact
candidates. Every compatibility claim must bind the actual versions, policy and tested operations.
The planning handoff records only documentation validation and the existing evidence-freshness gap.
