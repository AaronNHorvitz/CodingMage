# Decision 0010: Linux and Windows First Release

- **Status:** Accepted
- **Date:** 2026-08-23
- **Decision owners:** Repository owner and CodingMage maintainers
- **Supersedes:** The Linux-only release assumption in Decision 0008 and the Windows deferral in Sprint 18
- **Superseded by:** None

## Context

CodingMage has reproducible Linux x86-64 packaging and native Fedora evidence. The original plan
kept both macOS and Windows outside the first supported release until native evidence existed. The
current development environment can run hardware-accelerated x86-64 virtual machines and can
therefore produce genuine Ubuntu and Windows guest-operating-system evidence, but no Apple Silicon
host is available.

The release boundary must distinguish an unavailable hardware track from work that can be
implemented and qualified now. It must not convert cross-compilation, Wine, or platform-neutral
tests into native Windows evidence.

## Decision

The first supported CodingMage release requires:

- Linux x86-64, qualified on Fedora and Ubuntu LTS; and
- Windows 11 x86-64, qualified inside a genuine Windows guest operating system.

macOS and Apple Silicon are deferred, unsupported, and unqualified for the first release. Existing
Apple roadmap identifiers remain intact as a future platform track.

Windows support is not present merely because this decision is accepted. It becomes supported only
after the production adapters, package lifecycle, installed workflows, hostile fixtures, restart
recovery, and native guest evidence in Sprint 28 pass. Cross-compilation may supplement that
evidence but cannot replace it.

A VM result is native guest evidence only when CodingMage executes inside the target guest and the
guest's own filesystem, process, service, permissions, Git, installation, and recovery behavior are
exercised. Evidence must bind the guest version and architecture, source commit, package digest,
commands, results, and limitations.

## Alternatives Considered

- Keep a Linux-only first release. Rejected because Windows qualification is feasible in the
  current environment and is now an explicit product requirement.
- Keep macOS mandatory and wait for Apple Silicon. Rejected because it would block unrelated,
  implementable release work and encourage unsupported claims.
- Treat cross-compilation or a compatibility layer as Windows evidence. Rejected because neither
  exercises native Windows boundaries.

## Consequences

- Windows becomes release-blocking until Sprint 28 passes.
- Ubuntu LTS remains release-blocking alongside the existing Fedora matrix.
- Release archives, support statements, installation guides, and verification evidence must be
  platform-specific.
- Apple tasks remain open as deferred evidence but do not block the non-Apple release.
- Signing and independent human review remain separate release prerequisites.

## Verification

- Build and install CodingMage inside clean Fedora, Ubuntu LTS, and Windows 11 x86-64 guests.
- Exercise each supported guest's filesystem, process, service, Git, package, cancellation, and
  recovery boundaries.
- Run supervised, serial, parallel, integration, upgrade, rollback, and removal workflows from the
  installed artifacts.
- Verify that README, installation, unsupported behavior, release metadata, and package names make
  no macOS support claim.
- Reject the release candidate while any required Linux or Windows native-guest evidence is open.
