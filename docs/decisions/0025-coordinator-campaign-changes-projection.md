# Decision 0025: Coordinator Projection of Campaign Changes

- **Status:** Accepted for incremental native UI implementation
- **Date:** 2026-09-28
- **Decision owners:** CodingMage implementation worker under the owner's delegated UI scope
- **Supersedes:** None
- **Superseded by:** None

## Context

The Changes screen invoked Git directly with the selected repository path and parsed two
unversioned responses. Those reads did not prove that the repository and commit range still
belonged to the coordinator's reconciled campaign. A failed or malformed half-response could
also leave an incomplete summary. Decision 0016 and the native UI specification require the
coordinator command boundary for repository observations.

## Decision

Add read-only `codingmage campaign-changes --config ... --campaign ... --head ...`. It loads the
campaign specification and durable status, requires the caller's full head to equal the
reconciled head, and holds the configured repository identity. A hardened Git template verifies
that the specification's initial commit is an ancestor, then reads at most 501 commit summaries
and a four-MiB changed-path numstat. Malformed Git output is an error; more than 500 commit or
path entries is explicitly marked truncated. The command rechecks the held identity during the
reads and the durable head before returning a versioned projection with campaign/repository
identities, base, head, commits, paths and truncation flags. No diff body or provider content is
returned.

The UI uses the same bounded asynchronous sibling-binary worker as its other coordinator
observations. It checks the response against its selected campaign, repository diagnosis and
current status before rendering. A changed selection or head withholds the old summary. The
projection is evidence of repository objects only; it does not itself establish review,
acceptance or delivery.

## Limits

The private run-record scan, local configuration access, and remaining Show-command and state
catalogue work are still open under Task 36.3.2.2. A non-UTF-8 or control-bearing Git subject or
path is refused as malformed instead of being silently rewritten. Four-MiB Git capture bounds
may withhold a very large change summary rather than pretending it is complete. The UI's later
status poll detects head changes after the command's final check.
