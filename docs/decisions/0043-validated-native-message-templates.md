# Decision 0043: Validated Native Message Templates

- **Status:** Accepted for the incremental native catalogue implementation
- **Date:** 2026-09-30
- **Decision owners:** CodingMage implementation lane under Decision 0020
- **Supersedes:** None
- **Superseded by:** None

## Context

The bundled English catalogue already holds static Help, Settings, Workspace, Blockers,
Reports and Campaign text. Campaign summaries still combined backend identities, states and
counts with English text in Rust. A translated template that omits a campaign identity or
authority state would make the interface misleading even if it parsed as TOML.

## Decision

Named fields such as `{campaign_id}` are allowed only on declared catalogue keys. At load,
the catalogue rejects missing, unknown, repeated and malformed fields and rejects fields on
static messages. A translation may reorder declared fields. Formatting inserts each value
once as text; a value containing brace characters is never interpreted as another template.
The renderer continues to treat coordinator and repository values as inert content.

The Campaign screen uses this mechanism for selected identity, execution, observation,
active task, hold, utilization and mission summaries. Only the bundled English catalogue
ships in this increment. Its pseudo variants exercise expanded and right-aligned text in
local tests; they are layout stress inputs rather than translated releases. No dependency,
coordinator command or authority contract changes.

## Consequences

Call sites must provide every declared field. A mismatch in first-party code fails visibly
instead of silently dropping a state value. Date, number and duration locale rules, remaining
screens, full bidirectional layout and installed assistive-tool tests remain open under
Task 36.3.2.4 and Task 36.3.3.

## Verification

The focused catalogue test mutates required fields and malformed delimiters, checks field
reordering, and inserts brace-like backend text without reinterpretation. The full native UI
and workspace checks for this exact source are recorded in the matching Sprint 36 evidence.
