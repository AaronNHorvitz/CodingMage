# Monitoring

The monitor core provides ordered bounded status events, reconnect snapshots, unknown-versus-zero metrics, and content-minimized terminal and JSON views. Read commands have no mutation authority.

Pause, resume, stop-after-unit, and cancel controls require same-user authorization, exact run identity, and an idempotency key. Accepted controls are journaled. Cancellation applies only to proven owned descendants.

The current CLI exposes local `status`; transport wiring for the full live event stream and lifecycle controls remains part of live orchestration composition.
