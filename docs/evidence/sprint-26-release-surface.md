# Sprint 26 Release Surface Evidence

**Date:** 2026-08-26

**Implementation commit:** `26fb3ff4801203e322f767c50ecaea8581e72ce8`

**Scope:** Sub-tasks `26.1.1.1` through `26.1.1.3`, `26.1.1.5`, `26.1.2.1` through
`26.1.2.3`, and `26.1.2.5`

## Implemented Surface

- The release-scope register separates intended scope, implemented local behavior, native evidence,
  unsupported behavior, and release support across platforms, providers, task sources, project
  profiles, campaigns, controls, GitHub, and package lifecycle.
- The risk register records every known Sprint 26 qualification, platform, external-service,
  manual-review, signing, freeze, and publication blocker. None is waived for release.
- Public guidance now includes a pre-release support policy, platform/provider/repository
  compatibility matrix, known limitations, migration and retention procedure, synthetic first-run
  walkthrough, release notes, third-party notices, and an indexed operations surface.
- Top-level and command-specific CLI help are static, complete for every public command, and tested
  without repository, credential, or runtime-state access.
- Candidate construction now refuses dirty source, a missing, linked, repository-local, malformed,
  stale, mismatched, or unsupported review record, and missing, duplicated, escaping, linked, or
  digest-mismatched evidence before build execution.
- Candidate tooling emits two reproducible binary builds, a deterministic tracked-source archive,
  SPDX SBOM, dependency/license inventory, build manifest, provenance statement, release notes,
  notices, per-archive checksums, and an unsigned and unpublished release manifest.
- The installer requires the exact external review-record digest in the build manifest and rejects
  its mutation or omission.

## Validation

The implementation commit passed:

```bash
cargo fmt --all -- --check
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 scripts/docs_check.py
python3 -m unittest discover -s tests -p 'test_*.py'
git diff --check
```

The prescribed ten-outcome production-coordinator test passed in 194.54 seconds. The workflow
binary suite passed 13 tests; two sustained tests remained explicitly ignored behind
`CODINGMAGE_SUSTAINED_SOAK=approved`. All remaining workspace targets passed. Strict Clippy passed
with warnings denied. Documentation checks and all 35 Python governance tests passed. The focused
release-tool suite passed nine tests, including clean-source review binding, source-archive
reproducibility, manifest mutation, archive traversal, install, upgrade, rollback, removal, and
user-service lifecycle.

## Limitations

This evidence does not freeze a release commit, verify the final release candidate's complete
command and documentation surface, build or sign a final candidate, pass a live supervised unit,
run the live pilot or controlled soak, supply external platform or GitHub evidence, execute manual
fuzzing, obtain independent human review, or authorize publication. Sub-tasks `26.1.1.4`,
`26.1.2.4`, `26.1.3.6`, and `26.1.7.3` through `26.1.7.5` remain open with every downstream
acceptance criterion and gate they control.
