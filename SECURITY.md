# Security Policy

CodingMage is pre-release software. It coordinates tools that can modify source code, run
bounded commands, and interact with Git, so security reports are treated as potentially
sensitive even when no credential is involved.

## Supported Versions

No released version is currently supported. Security fixes apply to the latest commit on
the default branch until the project publishes a versioned support policy.

## Reporting a Vulnerability

Do not open a public issue for a suspected vulnerability. Use GitHub's private vulnerability
reporting feature for this repository. If that feature is unavailable, contact the repository
owner privately and request a secure reporting channel before sending technical details.

Include only the information needed to reproduce and assess the issue:

- The affected commit or version.
- The violated security boundary and expected behavior.
- A minimal reproduction using synthetic data.
- The observed impact and any known prerequisites.

Do not include credentials, tokens, private keys, personal data, proprietary source, live
exploit targets, or hidden model reasoning. Redact logs before attaching them.

The maintainer will acknowledge a complete report when it is received, establish a private
triage path, and provide status updates when practical. Response or remediation times are not
guaranteed while the project remains pre-release.

## Safe Disablement

Until executable releases exist, disablement means stopping any local CodingMage process and
revoking provider or GitHub credentials outside CodingMage. Future releases must provide a
documented stop command, deny new work after disablement, preserve an auditable terminal state,
and avoid deleting target worktrees or evidence automatically.

Security fixes must be delivered as reviewable commits. CodingMage must never self-patch,
self-merge, or weaken a gate in response to a report.
