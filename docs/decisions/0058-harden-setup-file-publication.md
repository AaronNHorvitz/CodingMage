# ADR 0058: Bind Guided Setup File Publication to the Repository

- **Status:** Accepted for the local writer correction
- **Date:** 2026-10-01
- **Decision owners:** CodingMage implementation

## Context

Guided Setup used a separate local candidate writer. It checked a destination with
path strings, removed a predictable candidate name before writing, and renamed the
candidate after validation. A linked parent could direct an authorization record or
export into the repository. Another file at the candidate name could be removed.
The report-export path already has a checked directory handle, a repository identity
check, create-new candidate, exact entry publication and an atomic no-overwrite path.

## Decision

Use that existing directory-handle writer for guided configuration, campaign,
authorization and document-export files. Configuration and campaign candidates still
pass their existing loaders before publication. Require an existing destination
parent for authorization and export files. Check the configuration destination and
new scratch/state roots against the canonical repository and existing ancestors
before creating roots. A linked parent into the repository is refused. The Setup
screen passes its selected repository to export instead of relying on a lexical
prefix check. The authorization writer enforces its already declared 1 MiB limit
before creating a file.

## Consequences

This removes the predictable-candidate deletion and static linked-parent bypass.
The Setup screen still performs these writes in the UI process. That remains a
section-2 command-boundary and render-thread gap under the native UI specification;
follow-up work must add public sibling `codingmage` commands and route the actions
through a bounded worker. This correction does not claim that boundary complete,
does not qualify a live provider or a human desktop workflow, and changes no
dependency, licence, coordinator or campaign execution policy.

## Verification

See [the local Setup writer evidence](../evidence/sprint-36-setup-writer-hardening.md).
The first exact-commit independent review found that the named candidate could
be replaced after validation, publishing rejected bytes. That finding and its
correction are retained in [Decision 0059](0059-publish-setup-documents-from-held-file.md);
this decision alone did not establish exact-byte publication.
