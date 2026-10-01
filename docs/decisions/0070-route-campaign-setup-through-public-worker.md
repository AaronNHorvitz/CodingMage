# ADR 0070: Route Native Campaign Setup Through the Public Worker

- **Status:** Accepted for local implementation; independent review pending
- **Date:** 2026-10-01
- **Decision owners:** CodingMage implementation

## Context

The native campaign form previously read the authorization record and wrote
the campaign specification on the render thread. The public commands
`setup-inspect-authorization` and `setup-write-campaign` now have bounded
independent exact-commit PASS reviews. Their authority checks belong to the
coordinator, and the UI specification requires long work off the render thread
and exact command previews beside actions.

## Decision

The native form submits one snapshot to its existing bounded worker. The
worker invokes the public inspection command with the exact diagnosed
repository ID, clean HEAD and committed task-source digest. It rejects an
unknown or mismatched receipt, builds the existing campaign specification from
that observed record digest, runs the existing verifier and serializes at most
1 MiB. It sends those candidate bytes through private stdin to the public
campaign writer. The writer makes its own fresh authority observations and
does not rely on the prior inspection as permission. The worker accepts only a
schema-1 write receipt matching the submitted repository, campaign, HEAD,
candidate byte count and SHA-256.

The UI stores both submitted argument vectors and keeps them visible in two
Show-command disclosures while the form is pending. Widgets are disabled
during that interval. If either exact command cannot be shown, the write
button is disabled while the fields remain editable. A selection generation,
repository binding and request identity discard stale responses. The current
diagnosis must still match the submitted head and task-source digest before the
written campaign is selected.
Failure, cancellation, malformed output or an uncertain write never triggers
an automatic retry; the owner is told to inspect the destination. Closing the
window remains separate from cancellation.

The legacy in-process form writer remains for compatibility with existing
library users and tests, but the native campaign action no longer calls it.
Native configuration and export writes, full state/depth coverage and
installed-desktop qualification remain open. The existing campaign-selection
loader still runs in the UI process after a confirmed write; moving that read
to the worker is a separate screen-wide selection change. No dependency or
licence changes are required.

## Verification

Focused Setup and full native test results, inventory disposition and
the exact reviewed commit are recorded in the Setup command evidence. These
local checks do not substitute for independent review, source-bound schema
checks, human trials, real-provider qualification or release approval.
