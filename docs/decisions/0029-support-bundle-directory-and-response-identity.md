# ADR 0029: Bind support exports to a directory handle and request identity

- **Status:** Accepted for the independent-review correction; re-review pending
- **Date:** 2026-09-29
- **Decision owners:** CodingMage implementation lane
- **Supersedes:** None
- **Superseded by:** None

## Context

A support bundle is an explicit local export, and its output must stay outside the selected
repository. An independent review of commit `37a133691b1f916271433e5eeeb9c36e6d41d1c2`
found that a pathname-only parent check could be invalidated before directory creation. It also
found that a response from an earlier export could be accepted for a later export after the same
campaign was reselected.

## Decision

The coordinator opens the validated output parent as a retained directory descriptor. It
checks that the requested parent still names that directory, creates the fresh 0700 child with
`mkdirat`, opens it without following a leaf link, and creates every bundle file relative to
that child descriptor with exclusive creation and no-follow flags. The final response is
withheld if the requested parent no longer names the retained directory. This uses the already
pinned `nix` dependency and the Linux descriptor boundary; the UI's earlier pathname check is
only an early refusal, never the final authority.

Each manual UI export has a session-local request ID and a campaign-selection generation. The
worker carries both through its response. Selection changes cancel older worker generations;
a support response with the wrong request ID is discarded before pending state can be consumed.
An uncertain or partial write is reported without automatic retry.

## Consequences

A renamed parent cannot redirect an export into the repository by replacing its original
pathname with a target alias. A pathname changed after creation may leave an export under the
retained directory's new name; the coordinator reports an uncertain result rather than claiming
the originally requested path succeeded. The operator must inspect that result before another
request. Existing source-bound, human, live-provider and release gates remain independent.

## Verification

The focused runtime tests passed 2/2: one covers existing and linked destinations, and one
swaps the output parent for a target alias after validation. The focused native Help tests
passed 2/2, including replay of a real prior bundle receipt after campaign reselection while a
new request is pending. The cumulative runtime library passed 131/131, the direct CLI
support-bundle process regression passed 1/1, the software-rendered native UI suite passed
99/99, and strict workspace Clippy passed. Python unittest passed 41/42; the sole failure is
CM-R01.6's eight source-bound input drifts, which this decision does not renew. Exact-commit
independent re-review remains pending; no human or release gate closes.
