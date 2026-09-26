# Configuration

## Planned Owner Involvement

[Decision 0014](../decisions/0014-owner-optional-engineering-team.md) specifies future supervised,
exception-only and hands-off modes, a mission charter and bounded decision-domain grants. These
are not supported version-1/version-3 configuration fields yet; do not add invented keys to the
current examples. Existing configurations retain their existing behavior and are not opted in.

The new contract must bind outcomes, scope, accepted engineering decisions, role profiles, limits,
expiry and noninteractive blocker dispositions. Involvement is separate from serial/parallel mode
and publication/promotion policy. Preflight must reject an interactive-only path when hands-off was
selected. Existing logins and qualified adapters are prerequisites, not permission to buy services
or obtain credentials. See the [PRD](../../PRD.md) and
[team contract](../architecture/autonomous-engineering-team.md).

## Authority Roots

Configuration version 1 requires absolute existing target, scratch, and state directories. Roots
may not overlap. The task source is a relative normal path inside the target repository, and parent
discovery defaults to disabled.

## Profiles And Gates

Agent profiles name a stable identifier, provider, and model. Gate commands contain an absolute
executable and literal argument vector; shell strings are not accepted. At least one profile and one
gate are required.

## Campaign Authority

Campaign specification version 3 binds the repository identity, absolute repository path, initial
commit, task-source digest, operator-authorization digest, campaign branch, allowed and denied
paths, providers, gate tiers, protected branches, aggregate limits, and publication ceiling.
Changing any bound value creates different authority and cannot silently resume old state.

`multi_agent` is optional. Its absence preserves the serial compatibility path. Its relevant fields
are:

| Field | Meaning |
| --- | --- |
| `execution_mode` | `serial` or `parallel`; serial requires one implementer. |
| `publication_mode` | `local_only`, `per_task_draft_pull_request`, or `campaign_draft_pull_request`. |
| `task_integration_policy` | `never`, `human_required`, or `auto_to_campaign_branch`. |
| `destination_promotion_policy` | `never`, `human_required`, or `auto_to_default_branch`. |
| `task_merge_strategy` | `squash` or `fast_forward_only`. |
| `concurrency` | Independent implementer, lead, reviewer, test, GitHub-writer, and integration-worker ceilings. |
| `resources` | CPU, memory, disk, process, timeout, heartbeat, retry, and provider-circuit ceilings. |
| `max_campaign_tokens` | Aggregate observed provider-token ceiling. |
| `max_task_tokens` | Per-task observed provider-token ceiling. |
| `max_task_correction_cycles` | Bounded gate, review, and CI correction count. |
| `max_follow_up_tasks` | Campaign-lifetime ceiling for sealed follow-up work admitted inside one source task's original path authority. |
| `integration_validation_interval` | Number of task integrations between cumulative gates and independent review. Default `5`; `1` validates every integration. Each integration still runs its affected gates and a fresh review of its own diff, and campaign finalization always runs the full cumulative gates and final review, so a larger interval trades intermediate whole-campaign checks for throughput without weakening the end state (Decision 0012). |

For five local pods, use `max_parallel_pods = 5`, `execution_mode = "parallel"`, five Claude
implementers, and physical resources sufficient for all five declared reservations. Team-lead,
GitHub-writer, and integration-worker counts are exactly one. The initial schema permits up to 16
reviewers and 64 test workers, but increasing a numeric ceiling does not grant paths, commands,
network, publication, or merge authority.

Remote publication additionally requires an exact `github` table naming an absolute `gh`
executable, authenticated account, host, owner, repository, remote, protected destination branch,
and unique required-check names. Publication mode, outer campaign publication, configuration
capabilities, and GitHub authority must agree exactly.

## External Capabilities

Network, feature-branch push, issue synchronization, and draft pull requests are separately denied
by default. Push, issues, or pull requests cannot be enabled while network is denied. Publication
mode must agree exactly with the grants.

Decision 0012's default of five cumulative integrations was accepted in the 2026-09-20 planning
reconciliation. Acceptance does not refresh earlier evidence; Task 25.2.4.6 tracks the stale binding.

Configuration contains references and policy, never raw credentials. Provider and GitHub CLIs use
their own existing authenticated stores only within their explicit adapter boundaries.

## Supervised Run Specification

One-unit execution additionally requires an absolute, regular, nonsymlink run-spec file. It names
one exact open dependency-ready sub-task, explicit relative owned paths, `candidate_only` or
`close_task` completion authority, absolute provider executables, model and effort selectors, and
Claude authentication mode. Unknown fields, relative executables, escaping paths, and malformed
identifiers fail closed. The run spec contains no credential value or monetary control.
`close_task` rejects every provider-reported limitation; `candidate_only` retains a reviewed
checkpoint without changing the canonical checkbox.

An optional `[repair]` table with `regression_gate = "configured-gate-N"` turns the unit into a
reproduce-before-repair unit. Before the implementer runs, the coordinator's gate runner executes
only that configured gate at the base commit and refuses the unit with
`codingmage.runtime.repair_not_reproduced` when it passes, because nothing was reproduced. After
the candidate's gates pass, the coordinator retains an integrity-protected repair receipt binding
the failing base observation and the passing candidate observation under
`<state>/gate-baselines/<repository_id>/`. Provider prose about a reproduction never satisfies the
requirement, and an unknown gate identity is a spec refusal. Campaign task authority does not yet
carry this requirement; it applies to supervised run specs.

An optional `context` string (at most 4096 bytes, no NUL) is appended to the implementer packet
as data; it grants no authority.

## Recipes

`codingmage recipe-instantiate --config <c> --recipe <recipe.toml> --template <run-spec.toml>
--task <TASK_ID> --output <new-run-spec.toml>` renders a versioned recipe into an ordinary
supervised run spec. The recipe declares a closed `kind` (`dependency_update`, `migration`,
`security_repair`, `documentation`, `tests`), bounded `parameters`, the `scope_paths` the unit
may own, `prerequisite_gates` that must exist in the configuration, an optional
`verification_gate` that becomes the unit's reproduce-before-repair gate, and a `rollback`
boundary. Only `git_recoverable = true` with no declared `external_effects` is admitted, because
the coordinator will not claim to undo effects it cannot reverse. Providers, authentication and
completion policy come from the operator's template, never from the recipe. An existing output
file is never overwritten. Recipe text reaches the implementer as data only.

The `existing_login` mode permits the provider CLIs to discover their own established login while
CodingMage supplies empty setting sources, strict empty MCP configuration, no network tools, and
file-only worktree permissions. The process receives only `HOME` and any present
`XDG_RUNTIME_DIR`, `XDG_CONFIG_HOME`, and `DBUS_SESSION_BUS_ADDRESS` references. CodingMage adds a
fixed `PATH=/usr/bin:/bin` for installed sandbox dependencies instead of inheriting ambient `PATH`.
Login-discovery values remain in memory and are never emitted or journaled; every other ambient
name, including API-key and token variables, is excluded.
