# First-Release Readiness Census (2026-09-20)

This census decides whether the local-preparation exception in
`TASKS.md` ("Remaining Dependency Order", steps 14–16) is engaged: local
contracts, fakes, and consumer tests in Sprints 29 through 31 may proceed
only when every earlier remaining first-release item has a recorded
unavailable prerequisite. Sources are `TASKS.md`, the cited evidence files,
and the 2026-09-20 session records. Nothing here closes any gate.

## Census

| Remaining first-release item | Unavailable prerequisite | Recorded in |
| --- | --- | --- |
| AC 5.1/5.2, Gates 5.1/5.2 (live Claude task, live-path secret handling) | Authorized live-provider disposable task | `docs/evidence/sprint-5-local.md`, `TASKS.md` Story 5.2 |
| AC 14.1, Gate 14.1 (isolated login/logout lifecycle) | Real isolated Fedora login session | `docs/evidence/sprint-14-local.md` |
| Gate 15.1, Sub-task 25.2.2.3 (authenticated GitHub workflows) | Approved test repository + authenticated campaign | `docs/evidence/sprint-15-local.md` |
| Task 16.2.1 (authenticated push, network recovery) | Authenticated remote + network-fault access | `docs/evidence/sprint-16-local.md` |
| AC 17.2, Gate 17.2 (controlled-target reconciliation) | Supervised evidence + remaining serial gates + owner-controlled review | `docs/evidence/sprint-17-*`, `TASKS.md` |
| Gate 20.2 (live correction unit) | Authorized live correction run | `TASKS.md` Sprint 20 Gate |
| Task 19.2.2, AC 19.2, Gate 19.1 (release review, signed candidate) | Independent review, risk decisions, signing identity | `TASKS.md` Story 19.2, External 4/8 |
| Sub-task 25.1.1.3 (sleep, restart, logout, quota, auth, network, disk) | Native lifecycle + authenticated-service faults | Planning handoff, `sprint-25-interruption-matrix.md` |
| Sub-task 25.2.3.4 (sleep, logout, shutdown) | Native lifecycle transitions | `docs/evidence/sprint-25-platform-faults.md` (incl. 2026-09-20 timing note) |
| Sub-task 25.2.4.6, AC 25.4, Gates 25.3/25.4 (evidence renewal) | External exact-commit construction record + package rebuild | `TASKS.md` Sub-task 25.2.4.6 (2026-09-20 note); binding hashes unchanged |
| Tasks 26.1.1.4/26.1.7.3–26.1.7.5 (freeze, supervised, pilot, soak, binding) | Rebuilt candidate (blocked above) + controlled soak target | `docs/evidence/sprint-26-frozen-qualification.md`, `TASKS.md` |
| Sub-task 26.2.1.4 (signing), Task 26.2.3 (fuzz, review) | Operator signing material; manual fuzz; independent review | `TASKS.md`, External 4/5/8 |
| Sub-task 26.2.2.4 (service lifecycle) | Running systemd user manager in the qualification session | `docs/evidence/sprint-25-platform-faults.md` (2026-09-20 note) |
| Sprint 27, External 7 (authorization, publication) | Explicit owner decisions; no publication effect yet | `TASKS.md`, Gates 26.4/27.x |
| Sub-task 28.1.1.4, Tasks 28.1.2/28.1.3, Story 28.2, External 2 (Windows) | Genuine Windows 11 x86-64 guest; Job-Object and NTFS behavior | `docs/evidence/sprint-28-windows-path-identity.md` (Open Items) |
| External 1 (Apple Silicon) | Deferred, non-blocking for first release | Decision 0010, `TASKS.md` register |
| External 3/4/5 (GitHub auth, review, fuzz) | Counterpart approval, reviewer, fuzz campaign | `TASKS.md` register |

Task 18.2.2 (macOS adapter) is deferred, not first-release. Task 25.2.1
remains open as a continuous coverage work queue (885 explicit gaps after the
2026-09-20 inventory refresh); its inventory sub-tasks are complete, and every
future batch maps its new surfaces, as this session's Windows batch did. It is
worked incrementally and does not gate local preparation.

## Verdict

Every remaining first-release item resolves to an unavailable external,
owner-only, counterpart, or native-guest prerequisite recorded above, or to
the continuous coverage queue. The local-preparation exception is therefore
engaged for dependency-ready local work: Story 29.1 first (host contract
schemas, transport, fakes, fixtures under its explicit dependencies), then
independent USTE or Muse preparation. No live integration, cross-repository
work, publication, or release claim is authorized by this census. Return to
earlier work the moment any prerequisite becomes available.
