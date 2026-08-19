# Sprint 4 Evidence

- **Status:** Passed on the Linux reference platform
- **Source commit:** `e3d7d48c0cdabe95fed04c0d7d55694fafc18c11`
- **Executed:** 2026-08-19 on Fedora Linux with Rust 1.95.0
- **Scope:** Bounded process runtime and provider-neutral fake adapters

## Commands

The complete 51-test Rust workspace, formatting, strict Clippy, rustdoc, architecture,
documentation, eight-test Python mutation suite, and `git diff --check` gates exited successfully.
Process integration tests ran serially to make process-group and parent-loss observations exact.

## Verified Process Behavior

- A pinned executable profile grants exact literal argument vectors and environment names. There is
  no shell-string or ambient-environment field. Response-file arguments, unknown vectors,
  ungranted environment names, symlinks, executable replacement, missing deadlines, and unbounded
  limits fail before target spawn.
- The Linux guard launches the target in its own process group, clears the environment, applies an
  open-file limit, monitors process count, parent PID plus start time, cancellation, output, and
  deadline, then terminates and reaps the exact group when required.
- Live fixtures covered metacharacters, standard input, expected and unexpected exit codes,
  reserved-looking target exit codes, stream digests, output overflow, timeout, cancellation,
  process-count overflow, executable replacement, and an independently killed coordinator parent.
- Stream results retain only the configured prefix while digesting and counting all observed bytes.
  Terminal metadata disambiguates real target exits from guard timeout, cancellation, parent-loss,
  and internal outcomes.

## Verified Adapter Behavior

- Provider-neutral operations cover probe, start, continue, cancel, usage, implementation, review,
  correction, verification, and administration roles.
- JSONL events require one schema version, exact session, contiguous sequence, one leading start,
  and one terminal final event. Unknown fields, malformed JSON, wrong sessions, out-of-order events,
  excessive text, and contradictory results fail closed.
- Deterministic scripts cover success, provider failure, quota, timeout, cancellation, malformed
  output, contradiction, usage, and multi-turn implementation and review.
- Adapter interfaces receive no repository, Git publication, merge, release, or task-state handle.
  Claimed commits, tests, merges, and releases remain explicitly untrusted values.

## Limitations

- Process containment is native Linux evidence. macOS and Windows implementations remain open.
- A target that deliberately creates a new session or process group can escape POSIX group
  containment. Later Linux hardening must add cgroup-level containment before untrusted live agents
  receive unattended authority.
- Provider fixtures are deterministic local fakes. Live Claude and Codex behavior is not claimed.
- The process fixture and parent-loss driver binaries are test artifacts and must be excluded from
  release installation manifests.
