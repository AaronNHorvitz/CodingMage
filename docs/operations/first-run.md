# Synthetic First Run

## Purpose

This walkthrough exercises local initialization, inspection, one supervised unit, monitoring,
interruption handling, evidence inspection, and removal using a disposable synthetic repository.
It is not a substitute for release qualification or permission to use a valuable target.

## Create A Disposable Target

Use an empty private directory outside the CodingMage source tree. Initialize Git, configure a
synthetic author identity for that repository, add a minimal `TASKS.md` with one open sub-task, and
commit it. Do not copy private source, credentials, or production data into this target.

Create separate private state, scratch, and configuration directories with mode `0700`, then run:

```bash
codingmage init --repo /absolute/synthetic-repository \
  --config /absolute/private/config.toml \
  --scratch /absolute/private/scratch \
  --state /absolute/private/state
codingmage doctor --config /absolute/private/config.toml
codingmage plan --config /absolute/private/config.toml
```

Inspect the generated deny-first configuration. Replace placeholder provider profiles only with
exact installed executables and existing-login model profiles you intend to test. Add literal gate
commands and create a version 2 run specification that names the exact synthetic task and owned
paths. Keep network, push, issues, pull requests, task merge, destination merge, and publication
denied.

## Execute And Monitor

```bash
codingmage run --config /absolute/private/config.toml \
  --spec /absolute/private/run.toml > /absolute/private/outcome.json
```

Watch lifecycle messages in that terminal. From a second terminal, use `doctor` for repository
readiness and inspect only the private content-minimized run state. A successful unit leaves a
coordinator-owned local branch for manual comparison; it does not change the active checkout,
merge, push, or publish.

## Stop And Resume

Prefer letting a unit reach its final JSON. For an intentional interruption, send `Ctrl+C` to the
terminal that owns the run, wait for owned-process cleanup, and preserve the retained state and
branch. Run `doctor`, inspect the terminal run identity, and resume only an explicitly recoverable
run with:

```bash
codingmage run --config /absolute/private/config.toml \
  --spec /absolute/private/run.toml \
  --run-id run-0123456789abcdef0123456789abcdef
```

CodingMage must refuse malformed, stale, cross-task, or mismatched recovery identity. Omitting
`--run-id` creates a distinct run.

## Inspect And Remove

Compare the retained branch to the recorded base, verify the target active checkout remained clean,
and retain only evidence required for diagnosis. Remove an installed package with the rootless
installer after stopping and removing any user service:

```bash
python3 scripts/install_release.py service-stop
python3 scripts/install_release.py service-remove
python3 scripts/install_release.py remove
```

Normal removal preserves configuration and runtime state. Review those directories separately;
never use `--purge-data` as part of a test cleanup unless permanent deletion is the explicit goal.
