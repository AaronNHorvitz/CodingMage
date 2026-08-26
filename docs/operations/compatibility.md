# Compatibility And Known Limitations

## Platform Matrix

| Platform | Build status | Native evidence | Release status |
| --- | --- | --- | --- |
| Fedora Linux x86-64 | Implemented | Local development and package evidence exists | Pre-release only |
| Ubuntu Linux x86-64 | Intended | Required native guest evidence is open | Unsupported |
| Windows 11 x86-64 | Intended | Required native guest evidence is open | Unsupported |
| macOS and Apple Silicon | Deferred | None claimed | Unsupported |

Linux execution relies on ordinary unprivileged filesystem, process, Git, and user-service
facilities. Platform evidence is not transferable: passing on Fedora does not prove Ubuntu,
Windows, container, remote-filesystem, or network-mounted behavior.

## Provider Compatibility

The implemented adapters target separately installed Claude Code and Codex CLIs through exact
structured invocation and existing local login stores. CodingMage does not bundle either provider,
accept raw API keys in configuration, or guarantee a provider model's availability, output quality,
quota, or stable command surface. A provider upgrade requires capability probing and fresh live
qualification before a release claim can rely on it.

## Repository Compatibility

The target must be an explicitly authorized, observable Git repository with the declared task
source and branch state. Linked authority roots, ambiguous ownership, unsafe worktrees, dirty active
checkouts outside an explicitly preserving workflow, unsupported task syntax, and repository
content that attempts to grant authority fail closed.

## Known Limitations

- The first package format is a Linux x86-64 tar archive with a rootless installer, not an RPM,
  Debian package, Windows installer, or macOS bundle.
- A supervised run requires an explicit version 2 run specification; it does not autonomously
  broaden owned paths or acceptance criteria.
- The campaign TUI is not implemented. Monitoring uses content-minimized CLI status, report,
  explanation, control, and stderr lifecycle surfaces.
- Existing-login provider sessions can be unavailable, rate-limited, or unable to produce valid
  structured output. CodingMage stops at independent per-stage and aggregate ceilings.
- GitHub support remains unqualified until its authenticated disposable-repository evidence passes.
- No public release, support lifetime, or migration guarantee exists yet.
