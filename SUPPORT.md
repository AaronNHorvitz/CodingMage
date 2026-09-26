# Support Policy

## Current Status

CodingMage is pre-release software. No version currently has a supported-release lifetime or a
compatibility guarantee. The latest commit on the default branch is the only maintenance target,
and even that target remains subject to incompatible correction while the release gates are open.

## Planned Version Policy

The first published release will document its supported operating systems, architectures, provider
CLI versions, configuration schema versions, and package lifecycle. A release remains supported
until the earlier of:

- the end-of-support date published with that release;
- a security condition that requires emergency disablement; or
- replacement by a later release after the announced migration window.

Support means that the maintainer accepts reproducible defect and vulnerability reports and may
publish reviewed fixes. It does not promise response times, hosted service availability, model
quality, provider uptime, or compatibility with undeclared platforms and tools.

## Upgrade And Migration

Pre-release state, configuration, and campaign schemas may change without an automatic migration.
Never point a new binary at irreplaceable state until its release notes and migration guidance have
been reviewed. Preserve the prior binary, configuration, state root, scratch root, and exact target
commit until upgrade verification passes. See the [migration guide](docs/operations/migration.md).

## Security Reports

Report suspected vulnerabilities through the private process in [SECURITY.md](SECURITY.md). Do not
put credentials, private source, personal data, or active exploit details in a public issue.

## Manual Support Bundles

`codingmage support-bundle --config <config> --campaign <campaign> --output <new directory>`
writes a redacted bundle on explicit request only. It contains the configuration view, campaign
status, blocker explanation, final report and mission status exactly as the corresponding
commands print them, plus `manifest.json` with file digests and a `README.txt` naming absent
records. It never contains logs, prompts, provider output, repository source, credentials or host
identities, never overwrites an existing directory, and nothing is uploaded; sharing the directory
is a manual decision by the operator.
