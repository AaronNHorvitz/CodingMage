# Sprint 5 Local Evidence

- **Status:** Local implementation complete; authenticated live containment gate remains open
- **Source commits:** `ea1d91dbe97d4307de5504b62da3df883d65f486`, `38c9cd3`, and `13fc191`
- **Executed:** 2026-08-19 on Fedora Linux with Rust 1.95.0
- **Scope:** Claude Code capability, session, packet, deny-first authority, result, and coordinator-owned commit contracts

## Commands

The 57-test Rust workspace, formatting, strict Clippy, rustdoc, architecture policy,
documentation checks, eight Python mutation tests, and `git diff --check` passed. A
content-minimized local probe identified Claude Code 2.1.136 and the required noninteractive,
JSON, JSON Schema, session, model, effort, permission, and bare-mode flags without inspecting
authentication state.

## Verified Behavior

- Capability probes pin the exact executable identity, clear the environment, bound output and
  runtime, and reject unsupported versions or missing required flags.
- Start and resume plans bind one UUID-shaped session to an exact run, task, agent profile,
  absolute worktree, branch, and source commit.
- Work packets include dependencies, owned relative paths, acceptance criteria, display-only test
  vectors, and prohibited actions. They label repository and tool content as untrusted data.
- Invocation uses bare mode, literal arguments, an empty ambient environment, an exact working
  directory, structured JSON output, a strict empty MCP configuration, a per-call budget ceiling,
  and the shared timeout, cancellation, process-count, file-descriptor, and output controls.
- Claude receives only scoped `Read`, `Edit`, and `Write` tools. Git metadata is denied explicitly;
  Bash, web access, subagents, skills, notebooks, MCP servers, unsandboxed command fallback, and
  tool-network domains are unavailable. Sandbox startup is configured to fail closed.
- A successful provider result may report that bounded edits are ready for coordinator-owned
  verification and commit creation. Exactly one of ready, commit, or blocker must be present.
- The coordinator-owned Git operation revalidates repository, worktree, branch, and parent;
  rejects every changed path outside declared ownership before staging; disables hooks, signing,
  helpers, editors, and ambient configuration; and proves the created commit has the expected
  direct parent. Live local fixtures preserve the active checkout and leave no undeclared path
  staged after refusal.
- Completion reports reject unknown fields, escaping paths, malformed commands, and reports that
  contain contradictory or absent dispositions.
- Provider result parsing distinguishes quota, context exhaustion, session disappearance,
  cancellation, timeout, malformed output, and general provider failure.

## Open Conditions

- The deny-first authority profile is structurally tested, but no authenticated Claude model was
  invoked to exercise the provider's Linux sandbox and built-in file-tool permission enforcement.
- No paid or authenticated model invocation was made. Bare mode intentionally excludes stored
  login state, while non-bare credential discovery has not been approved as part of the process
  environment contract.
- The disposable live task, malicious live repository campaign, authentication-redaction evidence,
  acceptance criteria, and Sprint 5 gates remain open. No provider charge or credential access was
  authorized for this local implementation evidence.
