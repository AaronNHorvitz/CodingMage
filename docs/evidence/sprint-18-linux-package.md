# Sprint 18 Linux Package Evidence

## Scope

This evidence covers a reproducible Linux x86-64 release candidate, SPDX dependency inventory, checksums, build provenance, rootless binary lifecycle, local-path rejection, and explicit platform contracts. It does not claim user-service lifecycle, artifact signing, native macOS execution, or native Windows implementation.

## Reproducibility And Contents

Two independent locked release runs each compile the binary twice and compare those binaries before packaging. The resulting complete archives were byte-identical with SHA-256 `46fe2463e30ac30ca12a026e87ab597ba3d8a584691b0dec07439056f3335513`.

The archive contains only:

- The stripped `codingmage` binary.
- Apache-2.0 license, README, and security policy.
- SPDX 2.3 SBOM.
- Per-file SHA-256 checksums.
- A manifest binding schema, version, source commit, source epoch, lockfile hash, binary hash, runtime-state absence, credential absence, and Linux-only evidence.

Rust source paths are remapped deterministically. Package creation rejects binaries containing the build repository, home directory, or their canonical aliases. A post-build string scan found no local path, provider-state path, test hook, or debug marker.

## Rootless Lifecycle

The corrected archive installed beneath an isolated user-owned prefix, verified its receipt, upgraded atomically, retained one previous binary, rolled back, verified again, executed `codingmage --version`, and removed idempotently. Unrelated data is preserved. Configuration and state are retained by default; purge requires a separate explicit flag and rejects linked or changed data paths.

## Platform Boundary

Linux declares process-group containment, user systemd, Secret Service references, physical filesystem identity, and Linux-native evidence. The macOS contract declares Darwin process groups, launch agents, Keychain references, and filesystem identity but explicitly has no native evidence. Windows records job-object, NTFS identity, user-task/service, Credential Manager, and console requirements while executable commands remain unsupported.

## Verification

Implementation commits: `4bab126`, `e031991`, `72aaef3`, `9961fa8`, and `f77d5c3`.

```text
python3 -m unittest tests.test_release_tools -v
3 passed; 0 failed

python3 scripts/package_release.py --output <first-output>
passed

python3 scripts/package_release.py --output <second-output>
passed

sha256sum <first-archive> <second-archive>
identical

cmp <first-archive> <second-archive>
byte-identical

install, verify, upgrade, rollback, verify, version, remove
passed under an isolated prefix

cargo test -p codingmage-platform --all-targets
3 passed; 0 failed

cargo test -p codingmage-cli --all-targets
3 passed; 0 failed
```

## Open Evidence

Sub-task `18.1.2.2` and `Gate 18.1` remain open because the packaged binary does not yet install and exercise the user service through start, stop, upgrade, rollback, and removal. Signing is also still a human release operation.

Sub-task `18.2.2.1`, `18.2.2.2`, and native macOS evidence remain open. The contract and literal launch/keychain plans do not substitute for native process, filesystem, launch-agent, credential, provider, monitoring, and recovery execution. Windows executable support remains intentionally unimplemented.
