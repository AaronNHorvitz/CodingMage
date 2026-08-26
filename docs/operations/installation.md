# Installation

## Build A Local Candidate

From a clean source checkout:

```bash
python3 scripts/package_release.py --output dist \
  --review-record /absolute/private/release-review.json
```

The packager first requires a clean source tree and an external exact-commit review record whose
declared evidence digests match repository files. It then performs two locked release builds,
rejects differing binaries, creates a deterministic Linux archive, and emits an SPDX SBOM,
dependency inventory, tracked-source archive, checksums, provenance, notices, release notes, and a
build manifest. The top-level release manifest binds both archives to the reviewed commit and marks
them unsigned and unpublished. The tool does not sign or publish an artifact.

The operator-controlled review record is a private JSON file outside the source repository:

```json
{
  "schema_version": 1,
  "source_commit": "40-lowercase-hex-characters",
  "reviewed_commit": "the-same-40-lowercase-hex-characters",
  "disposition": "approved_for_candidate_construction",
  "evidence": [
    {
      "path": "docs/evidence/example.md",
      "sha256": "64-lowercase-hex-characters"
    }
  ]
}
```

Every evidence path is repository-relative, unique, ordinary, unlinked, and digest-matched. The
packager refuses a dirty tree, mismatched commit, missing evidence, unknown record field, linked
record, or record stored inside the source repository before a compiler starts. This record
authorizes candidate construction only; it is not signing or publication authorization.

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
