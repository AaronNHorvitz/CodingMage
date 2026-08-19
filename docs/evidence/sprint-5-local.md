# Sprint 5 Local Evidence

- **Status:** Partially complete; live containment gate remains open
- **Source commit:** `ea1d91dbe97d4307de5504b62da3df883d65f486`
- **Executed:** 2026-08-19 on Fedora Linux with Rust 1.95.0
- **Scope:** Claude Code capability, session, packet, invocation, and result contracts

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
- Invocation uses literal arguments, an empty ambient environment, an exact working directory,
  structured JSON output, a strict empty MCP configuration, a per-call budget ceiling, and the
  shared timeout, cancellation, process-count, file-descriptor, and output controls.
- Completion reports reject unknown fields, escaping paths, malformed commands, and reports that
  contain both or neither a coherent commit and a truthful blocker.
- Provider result parsing distinguishes quota, context exhaustion, session disappearance,
  cancellation, timeout, malformed output, and general provider failure.

## Open Conditions

- Claude's CLI permission mode is not a kernel-enforced filesystem boundary. CodingMage therefore
  does not yet claim that a live Claude process can write only inside its assigned worktree or use
  only configured local tools.
- No paid or authenticated model invocation was made. Bare mode intentionally excludes stored
  login state, while non-bare credential discovery has not been approved as part of the process
  environment contract.
- The disposable live task, actual-commit reconciliation, malicious live repository campaign,
  authentication-redaction evidence, Story 5.2 authority task, acceptance criteria, and Sprint 5
  gates remain open.
