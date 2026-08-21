# Installation

## Build A Local Candidate

From a clean source checkout:

```bash
python3 scripts/package_release.py --output dist
```

The packager performs two locked release builds, rejects differing binaries, creates a deterministic Linux archive, and emits an SPDX SBOM, checksums, and a build manifest. It does not sign or publish the artifact.

## Rootless Lifecycle

```bash
python3 scripts/install_release.py install --archive dist/codingmage-0.1.0-linux-x86_64.tar.gz
python3 scripts/install_release.py verify
python3 scripts/install_release.py rollback
python3 scripts/install_release.py remove
```

Installation defaults to `~/.local`, uses atomic replacement, and retains one previous binary for rollback. Removal preserves configuration and runtime state. `--purge-data` is an explicit destructive retention decision and must not be used without reviewing those paths.

The archive is Linux x86-64 evidence only. It is not a macOS or Windows package.

Release-candidate and public-artifact verification must use the packaged binary rather than a
source-tree build. See [`Release`](release.md) for clean-clone construction, signing separation,
installed-candidate testing, owner authorization, and independent post-publication verification.
