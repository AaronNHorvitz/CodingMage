# Release

## Current Boundary

CodingMage is pre-release. This guide defines the approved target workflow; it does not claim that
an unchecked Sprint 26 or Sprint 27 item is complete. No model or coordinator may infer permission
to merge, tag, sign, or publish from a passing implementation, review, soak, or package test.

## Candidate Preconditions

Before freezing a release candidate:

1. Complete every locally implementable prerequisite through `Gate 25.4`.
2. Reconcile supported and unsupported behavior against the frozen source.
3. Pass the disposable ten-outcome and controlled-target ten-task one-pod qualification gates after
   the last reliability correction.
4. Pass formatting, strict lint, all-target workspace tests, documentation, architecture,
   traceability, supply-chain, adversarial, recovery, privacy, package, and clean-clone gates.
5. Confirm that every evidence record binds the candidate commit and current commands.
6. Keep the source tree clean and reject untracked release inputs.

## Candidate Construction

Build twice from separate clean clones using the pinned toolchain and locked dependencies. Produce
the Linux binary archive, source archive, checksums, SPDX SBOM, dependency and license inventory,
build manifest, provenance statement, supported-platform statement, unsupported-behavior statement,
and release notes.

Scan all source and artifacts for credentials, private paths, runtime state, target source, logs,
debug authority, unexpected executables, and undeclared files. Signing is an operator-controlled
operation. CodingMage and provider models never receive signing material.

## Installed-Candidate Verification

Install the candidate archive into a clean unprivileged user environment and test version output,
help, configuration validation, doctor, planning, supervised execution, serial campaign, monitoring,
controls, recovery, service lifecycle, upgrade, rollback, removal, and retention behavior. Run the
prescribed disposable ten-outcome campaign using the installed binary and bind evidence to the
package digest, not merely the source commit.

Manual fuzzing and independent human security and architecture review occur against the frozen
candidate. A code correction invalidates affected build, package, soak, review, and installation
evidence and requires a new candidate.

## Separate Human Decisions

The repository owner makes these decisions separately:

1. Approve the release-candidate branch for default-branch review.
2. Approve the exact default-branch merge.
3. Approve creation of an operator-signed version tag.
4. Approve public release publication and its exact assets.

Authorization must bind the repository, commit, tag, artifact digests, and supported scope. Missing,
stale, ambiguous, or broader authorization fails closed.

## Public Verification

After publication, download every public artifact through an independent clean path. Verify the tag,
commit, checksums, signatures, SBOM, provenance, archive contents, version output, installation
instructions, supported scope, unsupported behavior, security reporting, and license links. Install
the downloaded artifact and repeat smoke, doctor, disposable-run, recovery, and removal checks.

The release is not complete until public-artifact verification succeeds. A failed verification uses
the documented disablement or replacement process without rewriting published history.
