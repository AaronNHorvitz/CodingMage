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

## Optional User Service

Install the binary first, then bind the user service to one exact validated configuration and
campaign specification:

```bash
python3 scripts/install_release.py service-install \
  --config "$HOME/.config/codingmage/config.toml" \
  --campaign "$HOME/.config/codingmage/campaign.toml"
python3 scripts/install_release.py service-verify
python3 scripts/install_release.py service-start
python3 scripts/install_release.py service-stop
python3 scripts/install_release.py service-remove
```

These are separate operator actions. Installation reloads the user service manager but does not
start or enable the unit, and it never enables lingering. Start and stop use exact `systemctl
--user` argument vectors without a shell. Verification and removal fail closed if the installed
unit or its receipt changed. Remove the service before removing the binary; configuration and
runtime state remain preserved unless the separate `--purge-data` decision is supplied.

Release-candidate and public-artifact verification must use the packaged binary rather than a
source-tree build. See [`Release`](release.md) for clean-clone construction, signing separation,
installed-candidate testing, owner authorization, and independent post-publication verification.
