# Decision 0027: Bind Native Campaign Payloads to the Selected Authority

- **Status:** Accepted for local implementation
- **Date:** 2026-09-28
- **Decision owners:** CodingMage implementation worker under the owner's delegated UI scope
- **Supersedes:** None
- **Superseded by:** None

## Context

The native worker binds each request and response envelope to a configuration, repository,
campaign and generation. The `campaign-status` and `campaign-mission-status` JSON payloads also
carry campaign identities. Before this correction, a syntactically valid payload naming another
campaign could replace the selected campaign's observation. A mission payload could also carry a
different campaign-authority digest. An always-visible status bar would make that false state
more prominent.

## Decision

After schema parsing and before accepting a value, compare a non-null campaign status payload's
campaign ID with the currently selected specification. Compare a mission payload's campaign ID
and authority digest with that selection. A mismatch is a typed contract failure. Retain any
earlier observation as stale, clear dependent status projections by the existing failure path,
and visibly mark an earlier mission observation stale with a refresh instruction. The
coordinator's explicit `null` status and the documented no-charter response retain their
separate meanings. Repeated valid output recovers without changing campaign authority or task
state.

These checks are a consumer boundary on the existing machine-readable command output. They add
no UI policy grant, backend command, dependency or schema version.

## Verification and limits

A test uses a real coordinator status over a disposable fake-provider campaign, then injects
foreign status and mission payloads at the response boundary. It checks rejection, preservation
of the earlier observation, and recovery from correctly bound output. The original red test
receipt is retained privately. This local test is not real-provider or desktop qualification.
