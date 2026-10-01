# ADR 0069: Inspect Campaign Authorization Through the Public Command

- **Status:** Accepted for local implementation; bounded independent review passed
- **Date:** 2026-10-01
- **Decision owners:** CodingMage implementation

## Context

The native campaign form computes the authorization record digest by reading
that file on the render thread. The UI specification requires file and process
work off that thread and authority-bearing observations through the sibling
coordinator command. The public campaign writer verifies a candidate against
the record, but it cannot supply the digest needed to prepare that candidate.
The physical-file correction of Decision 0068 has a bounded independent PASS
at its exact commit; it can be reused for a read-only observation.

## Decision

Add `setup-inspect-authorization` to the public CLI. It requires an existing
configuration, exact observed repository ID, clean Git HEAD, committed task
source digest and absolute authorization record outside the repository. The
command holds no-follow descriptors for the configuration and record, refuses
their path or physical identity collision, reads at most 1 MiB of nonblank
UTF-8 authorization bytes, and revalidates the repository, configuration,
record and task source before returning. The result contains only schema
version, repository ID, HEAD, task-source digest, observed byte count and
SHA-256. No authorization text is returned. It writes nothing, grants nothing
and does not inspect credentials.

The native worker will use this observation to prepare candidate bytes off
the render thread, then submit them to the separately reviewed
`setup-write-campaign` command. The write command still makes its own fresh
observations; a prior inspection is neither permission nor a bypass.

No dependency or licence changes are needed.

## Verification

Disposable real-process tests cover a successful content-minimized
observation, stale repository/head/task source, a hard-linked configuration
record, a parent alias, a symbolic record link, malformed and oversized record
bytes and a repository-contained record. Exact results and remaining
native-worker work are recorded in the Setup command evidence.
The public CLI unit target passes 21/21, the disposable Setup target passes
7/7, strict workspace Clippy with warnings denied passes, and the full Python
suite retains only the unchanged CM-R01.6 source-bound freshness failure.
Formatting, documentation, architecture and inventory checks pass. These are
local results. Independent exact-commit review of
`3470b4f682e0de377bd13b61725f36ae0d1189e4` returned PASS with no open
finding for the public inspection command. That review does not cover the
subsequent native worker routing, human qualification or release.
