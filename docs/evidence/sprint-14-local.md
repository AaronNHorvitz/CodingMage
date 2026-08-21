# Sprint 14 Local Evidence

## Scope

This evidence covers locally implementable service specification, filesystem lifecycle, repository ownership, structured provider-capacity classification, and persisted retry scheduling. It does not claim a real isolated login/logout session.

## Service Lifecycle

The generated Fedora `systemd --user` unit binds an exact executable, configuration, state root, scratch root, restart delay, memory/task/CPU limits, `NoNewPrivileges`, read-only home access, strict system protection, control-group shutdown, and a bounded stop timeout.

CodingMage does not request root, write a system unit, enable the unit at boot, or enable lingering. Install writes only `codingmage.service` through a synchronized temporary file and atomic rename. Verify compares exact bytes. Uninstall is idempotent but refuses a symlink or human-modified unit. Start and stop remain explicit allowlisted lifecycle-plan steps for the later CLI/service transport.

One OS file lock is scoped to each repository identity. Duplicate owners fail immediately. Normal destruction explicitly unlocks, while a hard-killed owner releases its kernel lock without allowing a new process to adopt the old owner identity. Existing guarded-process evidence proves parent loss, timeout, cancellation, and normal completion clean exact process groups; the service unit additionally uses `KillMode=control-group`.

## Capacity and Retry

- Structured authentication, quota, rate-limit, overload, network, malformed-output, and terminal classifications take precedence over HTTP fallback status.
- Usage, remaining capacity, and reset fields remain optional rather than becoming zero.
- A known reset produces one deadline plus bounded deterministic jitter.
- An unknown reset uses nonzero capped exponential backoff.
- Retry attempt and next deadline are journaled and reconstructed after restart.
- Authentication, explicit terminal failures, and the configured attempt ceiling stop retries.
- A sustained millisecond-step fake-provider campaign proves no invocation occurs before its deadline or after terminal stop.

## Verification

Implementation commits: `399d7d1`, `ed40295`, and `8d7c7e6`.

```text
cargo test -p codingmage-service --all-targets
11 passed; 0 failed

cargo clippy -p codingmage-service --all-targets -- -D warnings
passed

systemd-analyze --user verify <isolated temporary codingmage.service>
passed through the native test fixture
```

## Open Evidence

`AC 14.1` and `Gate 14.1` remain open because this run did not create a separate Fedora login session and exercise actual start, logout, login, restart, and uninstall against that isolated user manager. Unit parsing and all local lifecycle components pass, but they are not a truthful substitute for that platform campaign.
