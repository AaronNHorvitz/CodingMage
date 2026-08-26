# Sprint 26 Package D Installed Ten-Outcome Evidence

**Date:** 2026-08-26

**Packaged source:** `04627990b4a2e2ea6d2c8cbc9cdcae13ea97bd82`

**Qualification-harness commit:** `a5b7c961cc1f1a68d9babe8f6f6694f3da5ba72e`

**Scope:** Sub-task `26.2.2.3`

## Bound Identities

| Item | SHA-256 |
| --- | --- |
| External candidate-construction review record | `d6b4e39cb6f9c6a380006c221dd801c45b634382d65e4023949b5cf4bfd040d0` |
| Linux x86-64 package archive | `8643bb20195259a4f7424abac3bb2200876d48c15b38819ef3348dce39b78045` |
| Tracked-source archive | `ffbdb2afd96412cedb444a5bba869279ff9c4813dd180a5590198055fd60c7b2` |
| Installed Package D binary | `47324d665a4a8ece63ef7abbed047057f8757fd6ea8f42dafd41ce2d8ecd5296` |

The installed binary digest was measured immediately before and after qualification and remained
identical. Package D was built from the packaged source shown above. The later harness commit adds
only an integration-test selector and does not change the installed product binary under test.

## Installed-Candidate Boundary

Ordinary workspace and CI execution continues to select Cargo's exact test-built binary. Installed
qualification requires both of these explicit values:

```text
CODINGMAGE_INSTALLED_QUALIFICATION=approved
CODINGMAGE_INSTALLED_BINARY=<absolute-installed-binary-path>
```

The harness refuses a missing pair, any other approval value, a relative path, a symbolic link, a
non-ordinary file, or a non-executable file. A focused test covers the ordinary default, valid
selection, malformed approval, incomplete pairs, relative paths, and symbolic links. This prevents
ambient configuration from silently replacing the normal test binary.

## Prescribed Campaign Result

The installed Package D binary executed the immutable prescribed ten-outcome production campaign:

```bash
CODINGMAGE_INSTALLED_QUALIFICATION=approved \
CODINGMAGE_INSTALLED_BINARY="$HOME/.local/state/codingmage/sprint-26-qualification/installed-d/bin/codingmage" \
cargo test -p codingmage-cli --test prescribed_campaign \
  production_coordinator_executes_prescribed_ten_outcome_schedule \
  --locked -- --exact --nocapture
```

Result: one passed, zero failed, completed in 17.21 seconds.

The exact schedule produced eight completed tasks, one truthful external blocker, one satisfied
deferral history, and ten accepted outcomes. It exercised a clean completion, deterministic gate
correction, independent review correction, unavailable dependency, explicit deferral trigger,
bounded malformed-report repair, provider-capacity pause, interruption after durable integration
intent, restart without implementation replay, authenticated stop-after-unit control, resume, and
the exact accepted-outcome ceiling.

The fixture verified bounded provider/process/storage observations, exact task checkboxes on the
isolated campaign branch, no pod-worktree residue, one recoverable campaign-root worktree, and an
unchanged clean active checkout at its original commit. No push, merge to a destination branch,
release, signing, network service, or publication effect occurred.

## Regression Validation

The source-tree binary completed the same schedule in 194.29 seconds before the installed run. The
complete post-change validation matrix then passed:

```bash
cargo fmt --all -- --check
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 scripts/docs_check.py
python3 -m unittest discover -s tests -p 'test_*.py'
git diff --check
```

The workspace run passed every active Rust target. Its workflow suite passed 13 tests while the two
separately guarded sustained qualifications remained explicitly ignored. Documentation checks and
all 35 Python governance tests passed. A case-insensitive tracked-content privacy scan also found
no prohibited downstream-project reference.

## Disposition And Limits

This evidence closes only Sub-task `26.2.2.3`. It does not qualify Package D as the final frozen
release candidate and does not satisfy the frozen-target supervised/pilot/soak matrix, final
cross-surface identity binding, reproducibility from separate clean clones, complete installed
command and lifecycle matrix, source freeze, signing, manual fuzzing, independent human review,
native Ubuntu or Windows evidence, authenticated GitHub evidence, or publication authority.
