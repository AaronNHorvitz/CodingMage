# Decision 0012: Batched Cumulative Validation by Default

- **Status:** Proposed
- **Date:** 2026-09-04
- **Decision owners:** Repository owner
- **Supersedes:** The implicit default of validating the whole campaign after every integration
- **Superseded by:** None

## Context

Serialized integration already distinguishes two verification scopes. Every integrated candidate
runs its affected gates and receives a fresh independent review of its own diff. Separately, at
`integration_validation_interval` merges, the coordinator verifies the cumulative campaign diff from
the manifest's source commit and requests an independent review of that whole diff
(`codingmage-runtime::team_integration`). Campaign finalization runs the full cumulative gates and a
final independent review unconditionally (`codingmage-runtime::team_campaign::finalize_team_campaign`).

The shipped default for the interval was `1`: the whole-campaign step ran after every single merge.
That makes the most expensive step scale linearly with task count. Controlled evidence from a
sibling unattended run (a 5,000-row roadmap driven by a single coding agent) showed exactly this
shape collapsing throughput from about 15 closed items per hour to about 1.3 when a global
artifact was regenerated per item, and recovering to about 50 per hour once regeneration was batched
behind completed work. CodingMage's interval is the same lever in code, and a default of `1` is the
unbatched setting.

Deterministic fixtures set the interval explicitly, so the default governs only operator campaigns
that omit the field.

## Decision

1. `DEFAULT_INTEGRATION_VALIDATION_INTERVAL` is `5`. A campaign that omits
   `integration_validation_interval` runs cumulative gates and whole-campaign review after every
   fifth integration.
2. Per-integration affected gates, per-candidate fresh review, the `1..=10_000` policy bounds, and
   unconditional finalization verification are unchanged. Operators who want the previous behavior
   set the field to `1`.
3. The constant is public so documentation, tests, and tooling reference one value.

## Alternatives Considered

- **Keep `1` and document the tradeoff.** Preserves the most-verified default but leaves every
  omitted-field campaign on the unbatched setting; the evidence shows that is the wrong default for
  the product goal of unattended roadmap progression.
- **A named long-campaign profile.** Adds configuration surface for a single integer that already
  exists.
- **A larger default such as `10`.** Fewer intermediate whole-campaign checks; `5` keeps a
  cumulative checkpoint within reach of a bounded correction cycle while removing the linear cost.

## Consequences

- Intermediate whole-campaign review happens less often for campaigns that omit the field; the
  final state is verified identically.
- A cumulative failure surfaces up to four merges later than before. Task-level gates and review
  still bound each merge, and refusal preserves candidates.
- Existing explicit configurations and all deterministic fixtures are unaffected.

## Verification

- `codingmage-campaign::team::tests::integration_validation_interval_defaults_to_batched_checkpoints`
  asserts the constant, the serde default when the field is absent, and policy-bound validity.
- `docs/operations/configuration.md` states the default and the finalization guarantee.
- Workspace formatting, documentation checks, and the campaign crate tests pass after the change.
