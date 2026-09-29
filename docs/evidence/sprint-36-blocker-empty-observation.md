# Sprint 36 blocker explanation empty observation

Task 36.3.2.2 remains open. The coordinator's `campaign-explain-blocker` command returns JSON
`null` when no durable campaign state exists. The UI now parses that as a successful, explicit
absent observation, just as it already does for campaign status and mission status. A retained
empty observation remains distinguishable from never requested, loading, malformed and stale.
The non-null explanation still needs to name the selected campaign; a foreign campaign payload
is a typed contract error and cannot replace the prior observation.

The real-process never-started campaign test asserts an observed `None` explanation without an
error. Contract tests assert a valid explanation round trip, `null` acceptance and malformed
non-object refusal. Reports use a neutral unavailable label when no explanation is present;
they do not claim a blocker has been cleared or that an absent record is a passed outcome.

This correction does not add a campaign command, state mutation, authority, provider, model or
dependency. The dedicated searchable Blockers screen, complete recovery language, human desktop
checks and live qualification remain open. Local verification on this tree: the focused
real-process empty-campaign case, foreign-payload regression and contract-parity case each pass;
`cargo test -p codingmage-ui --all-targets` passes 106 tests under labelled software GL;
strict workspace Clippy, formatting, documentation, architecture, inventory and diff checks
pass. The source-bound CM-R01.6 package renewal still requires its separate qualified-human
approval: the Python suite passes 41 of 42 tests, with only the existing eight-input
`test_multi_agent_evidence_binding_is_current` drift failure. The failing receipt remains in
private state. No independent or live acceptance is claimed.
