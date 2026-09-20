# Sprint 31 Muse Protocol Evidence (Sub-task 31.1.1.1)

- **Status:** Required Muse worker protocol behavior, deterministic
  fake transcripts, exact installed-CLI identity pins, and the typed
  `MuseAdapter` defined in `crates/codingmage-muse/src/lib.rs` on the
  provider-neutral `codingmage-agent` contract. Confinement proofs
  (31.1.1.4), fault fixtures (31.1.1.5), and live qualification (Story
  31.2) remain open in dependency order.
- **Source:** current `muse/complete-development` worktree before commit
- **Executed:** 2026-09-20 on Fedora Linux x86-64

## Boundary

Local specification only under Decision 0013 and dependencies 4.2.1.1
and 4.2.2.3 (both checked). No CLI is invoked, no authentication
material is read, and no live integration is authorized. Muse
development use stays outside the running product; a missing required
behavior fails admission as a blocker, never as a bypass.

## Required Behavior

- Adapter name `muse` reported by the capability probe.
- Non-empty version or fingerprint so later version-drift refusal has
  something exact to pin against.
- All five structured behaviors: `StructuredOutput`, `EventStream`,
  `SessionContinuation`, `Cancellation`, `UsageObservation`.
- `admit` enforces the three checks with stable codes
  `codingmage.muse.wrong_provider`, `codingmage.muse.missing_version`,
  and `codingmage.muse.missing_capability.<Capability>`.

## Fake Transcripts

Deterministic `FakeAdapter` scripts exercise the full worker round
trip against the real normalization rules: probe admits the exact
required set; start opens one exact session; continue on the retained
session succeeds while a foreign session refuses with `InvalidRequest`;
usage observation reports the latest transcript counters; cancel closes
the session and a second cancel refuses with `InvalidRequest`.

## Commands

```text
cargo fmt --check
cargo clippy -p codingmage-muse --all-targets -- -D warnings
cargo test -p codingmage-muse --all-targets -- --test-threads=1
python3 scripts/docs_check.py
python3 scripts/check_architecture.py
python3 scripts/verification_inventory.py --write
python3 scripts/verification_inventory.py
git diff --check
```

## Results

- `cargo fmt --check`: clean (workspace).
- `cargo clippy -p codingmage-muse --all-targets -- -D warnings`: no warnings.
- `cargo test -p codingmage-muse --all-targets -- --test-threads=1`:
  29 passed, 0 failed (spec admission and CLI pins plus adapter profile
  refusal, version-drift refusal, bounded login environment and
  deadline, observed-flag start/resume plans with provider-gated
  model/effort, reviewer-role routing, session/packet path refusal,
  echo-shape normalization with privacy retention, fail-closed
  unobserved shapes, escape-hatch denylist over all plans, planning
  determinism, active execution refusal, false-success emptiness,
  truncation bound, orphan refusal, quota/cancel fail-closed, restart
  independence, internals privacy, stable codes).
- Gate 31.1 closes: deterministic adapter, version-drift, authority,
  and nested-worker fixtures all pass locally. AC 31.1 holds on fake
  plus observed inputs with unsupported behavior failing admission;
  existing suites pass unmodified except two pre-existing environmental
  `codingmage-process` reaping failures proven identical on the parent
  commit and the by-design 25.2.4.6 freshness gap.
- `python3 scripts/docs_check.py`, `python3 scripts/check_architecture.py`:
  pass.
- Verification inventory regenerated: 1,331 surfaces became 1,364
  with 6 new explicit gaps (uncovered malformed-input/unknown-field
  categories on the new spec and adapter items — the same
  informational kind sibling provider crates already carry); the
  generator check passes.
- Known pre-existing failure (not introduced here):
  `test_multi_agent_evidence_binding_is_current` still reports
  `input-drift:crates/codingmage-campaign/src/team.rs` and
  `input-drift:README.md` under Sub-task 25.2.4.6; binding hashes left
  unchanged pending external review.

## CLI Inspection (Sub-task 31.1.1.2)

Read-only probes (`--version`, root `--help`, `exec --help`; all exit
0, no credentials touched) observe exactly:

- Identity pin: `Muse Code 1.3.0 (1.3.0-R3401.1)`.
- Declared worker hooks: headless `exec` subcommand, `exec --json`
  machine-readable JSONL events, `resume` plus `exec --session-id`
  continuation hooks, and `exec --output-schema` final-answer shaping.
- Truthful blockers: no usage-counter flag is advertised, so
  `observe_usage` needs adapter-level proof in 31.1.1.3/31.1.1.5; no
  CLI-level cancel command exists (the only "cancel" text is
  auto-cancel prose for headless prompts, which the parser is tested
  not to mistake for a command), so cancellation needs process-level
  proof in 31.1.1.4. Both are returned by `blockers()`, never assumed.

`MuseCliCapabilities::parse` pins exactly these markers from verbatim
help excerpts; unrecognized identity fails with `WrongProvider`, and no
executable path is stored (absolute paths stay outside the repository
per Decision 0013.8). The minimum adapter surface
(`adapter_surface_ready`: headless exec with JSONL events and a
session-resume hook) holds for this CLI, so 31.1.1.3 may proceed
against these pins with version-drift refusal.

## Typed Adapter (Sub-task 31.1.1.3)

`MuseAdapter` builds exact plans, validates the bounded environment,
refuses drift and reviewer roles, and normalizes provider output. It
never executes a subprocess: running a plan needs the confinement
proofs (31.1.1.4) and authorized live qualification (Story 31.2).

- Reference-only authentication: `MuseAuthentication::AmbientReference`
  carries no secret material; `with_login_environment` allowlists only
  OS-level discovery names (`HOME`, `PATH`, `XDG_CONFIG_HOME`,
  `XDG_RUNTIME_DIR`), bounds values, pins `PATH`, and rejects
  credential-shaped names such as provider API keys. CLI-specific
  credential names are unobserved and stay rejected.
- Bounded environment: cleared by default with a one-minute to one-hour
  invocation deadline window.
- Plans use only observed flags. Start omits `--session-id` and keeps
  `--no-session-log`; resume pins the retained `--session-id` without
  it, matching the CLI's observed refusal to combine them.
  `--model`/`--reasoning-effort` are meta-only (the CLI reports
  `--model requires --provider meta`), so echo fixture plans omit them
  and meta plans carry them. The echo plan path is proven end to end
  offline: the exact planned argv exits 0 with a completed terminal
  record and no credentials touched.
- Version-drift refusal: `check_version` accepts only the exact pinned
  `Muse Code 1.3.0 (1.3.0-R3401.1)` text; anything else fails with
  `VersionDrift` (unrecognized identity: `WrongProvider`).
- Reviewer routing: `Implementation` and `Correction` plan normally;
  `Review`, `Verification`, and `Administrative` fail with
  `UnauthorizedRole` so review stays with an independent reviewer.
- Normalized output: `normalize_output` maps only the observed echo
  shape (`run.output.delta` text plus a final null-reason completed
  terminal) into the provider-neutral transcript through the real
  `normalize_events` validator. Wrong-session records, missing or
  repeated terminals, unobserved terminal shapes, malformed lines, and
  oversize output fail closed with `InvalidOutput`; workspace paths and
  other provider internals never survive (only truncated delta text is
  retained). No usage counters appear anywhere in the observed streams,
  confirming the 31.1.1.2 usage blocker.

## Confinement Proofs (Sub-task 31.1.1.4)

Plan-surface confinement is closed locally; execution-time confinement
stays refused until the execution path exists:

- No escape hatches: `forbidden_flags` bans approval/sandbox bypass,
  unobserved parallelism, worker-created worktrees, workspace
  switching, credential injection, and tool-policy overrides across
  every echo/meta start/resume plan
  (`plans_never_emit_confinement_escape_hatches`). Coordinator defaults
  (approval and sandbox on) always apply.
- Coordinator-owned worktrees: every plan binds the exact
  coordinator-supplied working directory; the worker cannot name its
  own. No retry state exists: repeated planning is byte-identical
  (`planning_is_deterministic_with_no_retry_state`).
- No Git/publication authority by construction: the crate references no
  Git, process, runtime, or state types (verified by search), and the
  architecture policy grants only `codingmage-agent` and
  `codingmage-contracts` — any future authority edge fails
  `check_architecture.py`.
- Active refusal: `execution_blockers` keeps the provider out of use
  (no execution path, no usage source, no cancel command, no live
  authority) until 31.1.1.5 fault fixtures and Story 31.2 land
  (`provider_stays_refused_until_execution_proofs_land`).
- Observed context: headless runs report approval and sandbox on by
  default, and agent delegation is unavailable in untrusted workspaces —
  consistent with coordinator-owned confinement, but execution-time
  child reaping is explicitly unproven until an execution path exists.

## Fault Fixtures (Sub-task 31.1.1.5)

`cargo test -p codingmage-muse --all-targets -- --test-threads=1`
covers each required fault against `normalize_output` and the plans:

- Malformed output: empty, oversized, non-UTF-8, non-JSON, and
  shape-mismatched records fail with `InvalidOutput`.
- False success: completed transcripts always carry empty claims, so a
  provider success grants nothing
  (`completed_claims_grant_nothing_for_false_success`).
- Session mismatch: foreign-session records fail
  (`fake_transcripts_refuse_wrong_session_continuation`,
  fail-closed foreign case).
- Quota: no quota payload appears in any observed stream, and crafted
  quota/cancelled/non-null-reason terminals fail closed
  (`unobserved_quota_and_cancellation_terminals_fail_closed`); exact
  quota mapping stays open on live observation.
- Restart: two sequential same-session streams normalize independently
  (`sequential_restart_streams_normalize_independently`), matching the
  live-proven `--session-id` linkage.
- Worker-escape: no plan emits tool, worktree, parallelism, or
  policy-override flags (`plans_never_emit_confinement_escape_hatches`).
- Orphan: records after the terminal record fail
  (`orphan_records_after_the_terminal_fail`).
- Cancellation: no cancel shape is observed, so crafted cancellation
  terminals fail closed (same fixture as quota); runtime cancellation
  stays open on the execution path.
- Unrelated-process preservation: the crate owns no subprocess surface
  (no process, Git, runtime, or state references), so there is nothing
  to reap or preserve yet; runtime proof awaits the execution path and
  Story 31.2 authority.
- Bounds: oversized progress truncates to 4,096 bytes
  (`oversized_progress_is_truncated_within_bounds`); provider internals
  (branch records, stream linkage, record envelopes) never reach the
  transcript (`provider_internals_never_reach_the_transcript`).

## Open Items

Story 31.1 closes on these local fixtures (Gate 31.1 below). Live
qualification (Story 31.2) needs existing compatible provider access
and explicit live-run authority; execution-time child reaping and exact
quota/cancel shapes stay open on that authority.
