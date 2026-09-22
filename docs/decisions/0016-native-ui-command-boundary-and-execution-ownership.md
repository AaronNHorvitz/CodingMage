# Decision 0016: Native UI Command Boundary and Execution Ownership

- **Status:** Accepted for the Sprint 36 native UI workstream
- **Date:** 2026-09-21
- **Decision owners:** Repository owner (implementation choice delegated to the assigned UI agent)
- **Supersedes:** None
- **Superseded by:** None

## Context

The coordinator is the `codingmage` binary. `codingmage campaign` runs the campaign engine inside
that process under a kernel lock per repository, applies durable control intents at admission
boundaries, and exits with a JSON outcome when the campaign completes, pauses, blocks or is
cancelled. Status, blocker explanations and reports are projections of durable checkpoints read by
`campaign-status`, `campaign-explain-blocker` and `campaign-report`. Controls are create-once
intents written by `campaign-control`. The same binary is the process guard (`__process-guard`)
for every provider and gate subprocess, and repository authorization uses the binary's parent
directory to refuse a self-target. The native UI must present and request through this boundary
without duplicating coordinator logic or acquiring authority of its own.

## Decision

1. **The UI is a client of the CLI binary.** Every operation with repository, state or process
   authority runs the sibling `codingmage` executable as a bounded subprocess: `init`, `doctor`,
   `campaign-preflight`, `campaign`, `campaign-status`, `campaign-explain-blocker`,
   `campaign-report` and `campaign-control`. The UI parses the machine-readable stdout and the
   stable error code on stderr. It never links `codingmage-runtime`, `codingmage-git` or
   `codingmage-process` at runtime and never opens the coordinator's journals or locks.
2. **Read-only parsing reuses the libraries.** The work plan is parsed with `codingmage-plan`
   from the exact task-source bytes; configuration is validated with `codingmage-core`
   (`load_config`); campaign authority is authored and verified with `codingmage-campaign`
   (`CampaignSpec::verify`). These crates carry no authority; the UI's copies of backend JSON
   contracts use `deny_unknown_fields` and check `schema_version`, so a contract change fails
   visibly instead of rendering partial data.
3. **The coordinator owns execution.** Starting a campaign launches `codingmage campaign` in
   its own process group with stdin closed and stdout/stderr redirected to private files under
   `<state_root>/ui/launches/<campaign_id>/`. The UI records the process identity (pid and kernel
   start time) in a launch record. Closing the window neither signals nor waits for that process.
   Reconnecting reads the launch record, checks liveness from `/proc`, and shows the final JSON
   outcome when the process has exited. The UI never adopts, signals or attaches to a process it
   did not launch and never acquires the repository campaign lock.
4. **Controls go through `campaign-control`.** Pause, resume, stop-after-unit and cancel create
   UI-generated request identities that are retained locally so a retry replays the same request
   (idempotent) and a duplicate click while a request is pending is rejected. Resume writes the
   intent only; starting the coordinator again is a separate explicit action because a paused
   campaign's process has already exited.
5. **Admission is explicit.** Starting requires a campaign specification that verifies, an
   operator authorization record outside the repository whose digest matches the specification,
   a passing `campaign-preflight` report, and an acknowledgment of that report's exact SHA-256
   recorded by the UI. The admission is bound to the campaign authority digest, repository
   identity, initial commit and task-source digest. Any change to those inputs makes the admission
   stale and start is refused until preflight runs again.
6. **Every request is bound.** Backend requests carry the configuration path, repository
   identity and campaign identity they were issued for plus a generation counter. Results whose
   binding or generation no longer matches the current selection are discarded. Long work runs
   on one worker thread with a bounded queue; the UI thread only renders.
7. **Involvement modes.** Supervised, exception-only and hands-off modes have no backend
   contract in this source revision. The UI explains that they are unavailable and shows only the
   existing serial/parallel campaign authority and controls.

## Alternatives Considered

- **Link `codingmage-runtime` and call `campaign_status` in-process**: would require the UI
  binary to act as process guard and source root, duplicating the CLI's authority surface and
  coupling the UI to private runtime state layouts.
- **A long-running coordinator service with an IPC socket**: not present in this source revision;
  adding one would be a second orchestration surface outside Sprint 36.
- **Running `codingmage campaign` as a child that dies with the window**: violates the owner's
  requirement that closing the window is not cancellation.
- **Reading `checkpoint.json` files directly for status**: bypasses the authority revalidation
  performed by `campaign-status` and depends on `pub(crate)` schemas.

## Consequences

- The UI shows exactly what the CLI can report. Records the backend does not retain (for example
  reviewer finding text) are displayed as "not retained by the backend", never invented.
- The `codingmage` binary must be installed next to `codingmage-ui`; a missing binary is a real
  failure screen with the expected path and a way to select the executable in setup.
- Subprocess calls are bounded by a per-command timeout; the campaign launch is the only
  unbounded process and it is not waited on.
- Repository identity is derived from `doctor` output, not from path text, so moved or replaced
  repositories are detected by the coordinator's own checks.

## Verification

- Contract-parity tests serialize the real runtime types into the UI's models.
- Real-workflow tests run the actual `codingmage` binary against disposable repositories with fake
  providers: open, configure, preflight, admit, start, observe, pause, resume, stop, cancel,
  detach, reconnect and inspect outcomes.
- Negative tests cover stale bindings, duplicate requests, cross-project requests, malformed
  backend output, missing binary, killed coordinator and unreadable state.
