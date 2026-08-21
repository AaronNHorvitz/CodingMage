# Monitoring

The monitor core provides ordered bounded status events, reconnect snapshots, unknown-versus-zero metrics, and content-minimized terminal and JSON views. Read commands have no mutation authority.

Pause, resume, stop-after-unit, and cancel controls require same-user authorization, exact run identity, and an idempotency key. Accepted controls are journaled. Cancellation applies only to proven owned descendants.

The current CLI exposes local status and campaign status plus a content-minimized live stderr
activity stream. Production wiring of pause, resume, stop-after-unit, cancel, distinct deferral and
human-decision projections, and complete limit utilization remains unchecked Story 22.2 work.

The target campaign status contains phase, actor, task identity, model identity when available,
attempt, correction count, completed count, blocked count, deferred count, current limit
utilization, elapsed execution, and content-free reason codes. It must never expose prompts, source
text, filenames, provider prose, command output, unrestricted environment values, credentials, or
hidden reasoning.
