# Release Scope And Risk Register

## Scope Rule

This document enumerates the intended first-release surface and its current evidence state.
"Implemented" means code and deterministic local tests exist. It does not mean release-supported.
Only a frozen candidate that passes every applicable Sprint 26 gate can create a support claim.

## Intended First-Release Surface

| Surface | Intended scope | Current disposition |
| --- | --- | --- |
| Operating systems | Fedora and Ubuntu Linux x86-64; Windows 11 x86-64 after its native gate | Fedora local evidence exists; Ubuntu and Windows native evidence remain open |
| Provider adapters | Existing-login Claude Code implementer and Codex read-only reviewer | Implemented; final authenticated qualification remains open |
| Task source | One explicitly selected repository-local `TASKS.md` using the supported checklist grammar | Implemented and deterministically tested |
| Project profiles | Explicit task source, owned paths, and literal local gate commands | Implemented; no implicit repository authority |
| Supervised mode | One exact task in one coordinator-owned branch and worktree | Implemented; final candidate qualification remains open |
| Campaign modes | Durable serial and bounded one-to-five-pod parallel execution with serialized integration | Implemented; valuable-target unattended qualification remains open |
| Operator controls | Read-only status/report/explanation plus exact pause, resume, stop, cancel, blocker, deferral, and approval controls | Implemented and locally tested |
| GitHub | Optional issue, task-branch, draft-PR, exact-commit CI, and serialized integration adapter | Implemented with fakes; authenticated live evidence remains open |
| Package lifecycle | Reproducible Linux archive, rootless install, verify, upgrade, rollback, service lifecycle, remove, and explicit data purge | Implemented locally; final frozen-candidate lifecycle evidence remains open |

The complete command and authority surfaces are described by the [operations index](README.md),
[configuration guide](configuration.md), and [unsupported behavior](unsupported.md).

## Explicitly Unsupported Or Unqualified

- macOS and Apple Silicon;
- native Windows behavior until the Windows guest matrix passes;
- native Ubuntu behavior until its guest matrix passes;
- provider SDKs, arbitrary model endpoints, Jira, and Azure DevOps;
- unrestricted model shell, model-held credentials, and model-created authority;
- automatic default-branch merge, release signing, package publication, paid-resource creation, or
  infrastructure administration;
- manual fuzz, independent-review, signed-artifact, and public-release claims until those exact
  gates run; and
- compatibility with unlisted provider CLI versions or repository task formats.

## Open Release Risks

| ID | Condition | Pre-release disposition | Release disposition |
| --- | --- | --- | --- |
| R26-01 | Two installed Package C supervised units did not reach a passing final review | Superseded by the bounded Package D supervised pass; failure evidence remains retained | Blocking until the final candidate repeats the pass |
| R26-02 | Package D passed the controlled live-completion, external-blocker, and quota-pause pilot; the final frozen-target run remains open | Controlled pilot resolved | Blocking until the final frozen-target matrix passes |
| R26-03 | The required controlled approximately ten-task soak has not passed | Open | Blocking |
| R26-04 | Ubuntu native evidence has not run | Open external platform evidence | Blocking for an Ubuntu claim |
| R26-05 | Windows 11 native evidence has not run | Open external platform evidence | Blocking for a Windows claim |
| R26-06 | Authenticated GitHub behavior has not completed its disposable-repository matrix | Open external-service evidence | Blocking for a GitHub support claim |
| R26-07 | Manual fuzzing remains deliberately deferred | Accepted only during development | Blocking |
| R26-08 | Independent human security and architecture review has not occurred | Open external review | Blocking |
| R26-09 | Operator-controlled artifact signing has not occurred | Open human action | Blocking |
| R26-10 | No release publication has been authorized | Deliberately prohibited | Blocking by design |
| R26-11 | Final source, package, evidence, and support identities are not frozen | Open until the last correction | Blocking |

No listed risk is waived for release. A correction that affects a candidate invalidates every
dependent build, package, installed, soak, review, and signature record.
