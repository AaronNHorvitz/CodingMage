# Monitoring

## Status Surfaces

The monitor core provides ordered bounded events, reconnect snapshots, unknown-versus-zero metrics,
and content-minimized terminal and JSON views. Read commands have no mutation authority.

```bash
codingmage campaign-status --config /absolute/config.toml --campaign /absolute/campaign.toml
codingmage campaign-report --config /absolute/config.toml --campaign /absolute/campaign.toml
codingmage campaign-explain-blocker --config /absolute/config.toml --campaign /absolute/campaign.toml
```

Team status includes all active pod identities, lifecycle state, heartbeat sequence, scheduler and
integration queue position, resource utilization, terminal reason, and final report state. It omits
prompts, source, filenames, provider prose, command output, unrestricted environment values,
credentials, and hidden reasoning.

## Controls

The owner-optional extension adds mission/milestone and acceptance coverage, role and decision
identities, involvement mode, intervention count, no-progress reasons and separate
engineering/delivery dispositions across Sprints 32 through 35. Sprint 32 now provides the mission
charter part locally: `campaign-mission-admit --mission <charter>` binds one charter to the exact
campaign authority digest, `campaign-mission-status` reports generation, involvement mode, expiry,
revocation epoch and decision counts, `campaign-mission-revoke --request <id>` is an irreversible
authenticated revocation checked before every effect, and `campaign-mission-answer` records the
owner's answer to one pending decision in supervised or exception-only mode. Hands-off observation
is optional; disconnect does not stop the campaign or grant new authority. Pending external
prerequisites remain visible without repeated interactive prompts. Explicit pause, cancellation,
expiry and revocation still apply. None of this is live no-intervention qualification.

Pause, resume, stop-after-unit, and cancel require same-user authorization, exact campaign identity,
and a create-once request ID:

```bash
codingmage campaign-control --config /absolute/config.toml \
  --campaign /absolute/campaign.toml --action pause --request pause-1
codingmage campaign-control --config /absolute/config.toml \
  --campaign /absolute/campaign.toml --action resume --request resume-1
codingmage campaign-control --config /absolute/config.toml \
  --campaign /absolute/campaign.toml --action stop_after_unit --request stop-1
codingmage campaign-control --config /absolute/config.toml \
  --campaign /absolute/campaign.toml --action cancel --request cancel-1
```

Accepted controls are journaled and exact replay is idempotent. Pause stops new admission, stop waits
for bounded active work to checkpoint, and cancel terminates only proven owned descendants.

The CLI also emits one sanitized stderr activity line per lifecycle transition while reserving
stdout for the final JSON result. An attachable full-screen TUI remains a future enhancement.
