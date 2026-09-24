# 0018: Mission Charter and Owner-Involvement Contracts

| Field | Value |
| --- | --- |
| Status | Accepted for local implementation under Decision 0017 |
| Date | 2026-09-24 |
| Scope | Sprint 32 (CM-R02, CAP-09, 33, 34, 35, 36, 42) |
| Authority effect | None at admission time; a charter can only narrow what a campaign may do |

## Context

Decision 0014 and the [team contract](../architecture/autonomous-engineering-team.md) require an
immutable authenticated mission charter, three involvement modes, noninteractive decision
dispositions and revocation/expiry that survive restart. The existing coordinator already owns
campaign authority (`CampaignSpec` and its canonical digest), durable integrity-protected
controls, journals and typed lead dispositions. The question was where the new contract lives and
how it binds to that authority without creating a second engine or state store.

## Decision

1. **A separate charter file bound to the campaign digest.** `MissionCharter` lives in
   `codingmage-campaign` next to `CampaignSpec`. It is loaded from its own TOML file and must name
   the exact `campaign_id`, `repository_id`, `initial_commit`, `task_source_sha256`,
   `operator_authorization_sha256` and the campaign `authority_sha256`. The campaign bytes do not
   change, so every existing campaign keeps its authority digest and legacy behavior. A charter
   for a different campaign, commit or authorization is unbound and refused before any effect.
2. **Deny-first, closed schemas.** Every charter structure uses `deny_unknown_fields` and closed
   enums. Decision domains name a class, approved alternatives, scope paths that must be
   campaign-permitted, a maximum risk, required gate tiers that must exist in the campaign, and
   an escalation disposition. The command registry is a list of existing gate profile names,
   never shell text. A charter cannot grant anything the campaign does not already permit.
3. **Involvement is independent of delivery.** `InvolvementMode` (`supervised`,
   `exception_only`, `hands_off`) only decides whether an undelegated choice may become an owner
   request. Integration, publication and promotion authority continue to come from
   `MultiAgentPolicy` and the approval documents. A hands-off charter that configures an
   `ask_owner` escalation is invalid rather than silently downgraded.
4. **Decisions are evaluated, never negotiated.** `evaluate_decision` is a pure function over
   the verified charter, the untrusted proposal and coordinator observations (time, revocation
   epoch, budgets, prior attempts). Only `permitted_choice` permits an effect. Expired or revoked
   missions, class/path/risk/gate expansion, budget exhaustion and repeated no-progress produce
   typed holds. Hands-off never yields `decision_needed`.
5. **Durable mission state reuses the integrity-document and control pattern.** The runtime
   persists a `team-mission.json` document beside the existing control documents, bound to the
   campaign authority digest, mission digest and identities. It records the authority generation,
   revocation epoch, expiry, decision records and owner answers, and is re-read before every
   effect the same way `team-control.json` is. Revocation is an authenticated, irreversible,
   idempotent control request. Observer disconnect is not a control.

## Consequences

- Old configurations run unchanged; without a charter no involvement mode is claimed and the
  existing human-decision blocker behavior is preserved.
- The native workspace can show an involvement mode only when the runtime reports a bound
  charter, matching PRD CM-UI constraint 5.
- Re-issuing a charter requires a higher `generation`; a lower or equal generation with different
  content is refused, so old state can never regain authority implicitly.
- Real no-intervention qualification (Sprint 35) remains open; these contracts are deterministic
  local scope only.

## Alternatives Considered

- Embedding the charter in `CampaignSpec` would change the authority digest of every existing
  campaign and re-bind durable controls; rejected.
- Free-text delegation ("do anything necessary") cannot be evaluated deterministically; rejected
  by the team contract.
- Using the journal alone for decisions would leave no verified projection to check before an
  effect; the integrity document plus journal intent/observation records is the existing pattern.
