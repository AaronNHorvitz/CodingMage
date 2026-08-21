# Configuration

## Authority Roots

Configuration version 1 requires absolute existing target, scratch, and state directories. Roots may not overlap. The task source is a relative normal path inside the target repository, and parent discovery defaults to disabled.

## Profiles And Gates

Agent profiles name a stable identifier, provider, and model. Gate commands contain an absolute executable and literal argument vector; shell strings are not accepted. At least one profile and one gate are required.

## External Capabilities

Network, feature-branch push, issue synchronization, and draft pull requests are separately denied by default. Push, issues, or pull requests cannot be enabled while network is denied. Publication mode must agree exactly with the grants.

Configuration contains references and policy, not raw credentials. Native credential-store integration remains outside the current CLI workflow.

## Supervised Run Specification

One-unit execution additionally requires an absolute, regular, nonsymlink run-spec file. It names one exact open dependency-ready sub-task, explicit relative owned paths, `candidate_only` or `close_task` completion authority, absolute provider executables, model and effort selectors, and Claude authentication mode. Unknown fields, relative executables, escaping paths, and malformed identifiers fail closed. The run spec contains no credential value or monetary control. `close_task` rejects every provider-reported limitation; `candidate_only` retains a reviewed checkpoint without changing the canonical checkbox.

The `existing_login` mode permits the provider CLIs to discover their own established login while CodingMage supplies empty setting sources, strict empty MCP configuration, no network tools, and file-only worktree permissions. The process receives only `HOME` and any present `XDG_RUNTIME_DIR`, `XDG_CONFIG_HOME`, and `DBUS_SESSION_BUS_ADDRESS` references. CodingMage adds a fixed `PATH=/usr/bin:/bin` for installed sandbox dependencies instead of inheriting ambient `PATH`. Login-discovery values remain in memory and are never emitted or journaled; every other ambient name, including API-key and token variables, is excluded. `bare` retains Claude's stricter boundary but requires separately implemented external credential-helper integration before it is usable in production.
