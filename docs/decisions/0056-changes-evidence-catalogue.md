# ADR 0056: Bound Changes Copy to the Native Message Catalogue

- **Status:** Accepted for the Changes screen increment
- **Date:** 2026-10-01
- **Decision owners:** CodingMage implementation

## Context

The Changes and reviews destination still built most of its headings, evidence
states and record summaries from English literals. Its delivery paragraph also
said nothing was pushed even when the configured campaign policy permitted
draft story pull requests. The screen has no bound receipt from which to infer
a publication outcome.

## Decision

Move the destination's authored copy into the validated English catalogue.
Require named fields for the displayed policy, observation age, commit and run
identities and counts. Render repository and coordinator strings as inert text.
Show the configured publication policy and state that this screen has no
verified delivery receipt; make no claim that a permitted publication did or
did not occur. Reuse the existing catalogue review and gate labels for the
larger run-record view so its wording matches the selected-task view.

Keep the coordinator command boundary and refresh controls unchanged. An empty
run-record projection means this view cannot infer completion, not that no
work occurred anywhere. The catalogue remains English only; synthetic expansion
and right alignment are stress checks, not full locale or RTL support.

## Consequences

The UI reports policy and evidence separately. No action grants publication
authority, and this increment adds no dependency, credential access or runtime
adapter. Dynamic locale formatting, complete bidirectional navigation,
installed screen-reader validation and human/live qualification remain open.

## Verification

See [the Sprint 36 message-catalogue evidence](../evidence/sprint-36-message-catalogue.md).
