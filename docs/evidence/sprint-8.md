# Sprint 8 Evidence

- **Status:** Passed locally
- **Source commit:** `663676541c1c85cb7dfef4b09fac61dfb8c96a46`
- **Executed:** 2026-08-19 on Fedora Linux with Rust 1.95.0
- **Scope:** Trusted gate registry, concurrent runner, progress, evidence, and mutation checks

## Commands

The 74-test Rust workspace, formatting, strict Clippy, rustdoc, architecture policy,
documentation checks, eight Python mutation tests, and `git diff --check` passed. Six live gate
integration tests executed through dedicated bounded-process guard and target fixtures.

## Verified Behavior

- Trusted in-process definitions cover Tier 0 through Tier 4, deterministic triggers, required and
  optional policy, exact process profiles, declared resource conflicts, and mandatory assertions.
  Gate definitions are intentionally not deserializable from model output.
- A greedy deterministic schedule runs gates concurrently only when declared resources are
  disjoint. Required failures cancel other live gates; later batches receive explicit
  `prior-required-failure` evidence instead of disappearing.
- Required unavailable gates block. Optional unavailable gates are skipped only by exact explicit
  policy. Unknown skip identifiers fail closed.
- Progress emits exactly one content-free scheduled event and one terminal event per gate with a
  contiguous run-local sequence. No command output enters progress records.
- Process results retain complete stream counts and digests plus bounded prefixes only in memory.
  Gate evidence persists no output content and records truncation and descendant cleanup.
- Signed evidence binds source commit, tier, requirement, trigger, resource set, executable
  identity, arguments, working-directory hash, environment names, stdin hash, deadlines, output,
  process and descriptor ceilings, expected exits, assertions, timestamps, outcome, output
  digests, and cleanup.
- Exit zero cannot pass without every expected assertion. Representative independent mutations of
  gate identity, source commit, outcome, output observation, deadline, expected exits, assertions,
  and evidence digest all fail integrity verification.

## Limitations

- The fixture and guard binaries are test-only artifacts and must remain excluded from release
  installation manifests.
- Tier membership and triggers are trusted coordinator configuration. Automatic project-profile
  selection is implemented in later routing and project-adapter sprints.
