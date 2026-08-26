# Sprint 26 Package F Candidate Evidence

**Date:** 2026-08-26

**Source:** `a6546c6f482d6159857b96e106e804bfb92036f3`

**Machine-readable record:**
[`sprint-26-package-f-candidate.json`](sprint-26-package-f-candidate.json)

## Construction And Reproducibility

Package F was admitted by an external exact-commit candidate-construction record with SHA-256
`766b2f1cbff63d6818bc181bdee36c0580275d291b6ef9b04a3b2d10a33d30e3`. The primary clean checkout
and two separate no-hardlinks clean clones each built the candidate. Every binary archive, source
archive, checksum file, and release manifest compared byte-for-byte across all three builds.

| Artifact | SHA-256 |
| --- | --- |
| Linux x86-64 archive | `8674091a421f6c979709122fd3c1b1b88f69b951fb43f2294bdc7846f5dc33f4` |
| Tracked-source archive | `39a0350ad473f0c7bbf92b7f83ce401975920f428bbe9b577f4d3191d9795685` |
| Installed executable | `47324d665a4a8ece63ef7abbed047057f8757fd6ea8f42dafd41ce2d8ecd5296` |

The packager emitted the binary and source archives, checksums, SPDX SBOM, dependency/license
inventory, build manifest, provenance, notices, and release notes. The release manifest explicitly
records that the candidate is unsigned and unpublished.

## Artifact Scan

The tracked `scripts/scan_release.py` scanner compared the source archive to all 216 Git-tracked
source files and enforced the exact 12-file binary layout. `bin/codingmage` was the sole executable.
Every package checksum matched. The scan found zero high-confidence credential signatures, actual
private local paths, runtime/build residue, and undeclared files.

The scanner itself passed positive and adversarial tests for credential, private-path, executable,
dirty-source, and undeclared-file refusal. Its report exposes only counts and digests.

## Installed Qualification

The rootless installer installed and verified Package F in a fresh private prefix. Version output,
top-level help, and all 15 command-specific help surfaces passed. The installed binary passed
`doctor`, `plan`, and `status` against the exact clean frozen no-hardlinks target. It reported the
first unresolved Mac-only prerequisite truthfully without modifying that target.

Package F then passed the prescribed disposable ten-outcome production campaign in 17.11 seconds.
The executable digest was identical before and after. The schedule reached eight useful
completions, one truthful external blocker, one satisfied deferral history, and exactly ten accepted
outcomes while exercising gate and review correction, malformed-report repair, capacity pause,
durable interruption, restart without implementation replay, stop-after-unit, resume, and exact
ceiling enforcement. The active checkout and destination branch were unchanged.

The installed executable is byte-for-byte identical to the Package D executable that passed the
fresh supervised unit and three-outcome live/blocker/quota pilot. Package F also passed
same-artifact upgrade, verify, rollback, removal while preserving unrelated prefix data, and
deterministic reinstall without creating configuration or runtime state implicitly.

## Current Boundary

This evidence closes the clean-clone reproducibility, generated-artifact, artifact-scan,
unprivileged-install, installed command/runtime, deterministic reinstall, and installed disposable
campaign requirements listed in `TASKS.md`. It does not freeze the final source/evidence commit or
close the frozen-target supervised, pilot, and controlled-soak matrix. Native user-service execution
was not attempted outside private qualification roots. Signing, manual fuzzing, independent human
review, native Ubuntu and Windows evidence, authenticated GitHub evidence, merge, tag, release, and
publication remain open.
