# Sprint 28 Windows Path-Identity Evidence

- **Status:** Sub-tasks `28.1.1.1` through `28.1.1.3` implemented with hostile
  fixtures; native probing and guest qualification remain open
- **Source:** `0953968530f364cfdc0dd46f9a372a2b09f0408e` on `muse/complete-development`
- **Executed:** 2026-09-20 on Fedora Linux x86-64

## Boundary

`crates/codingmage-platform/src/windows.rs` provides host-independent pure
validation only: Windows volume and path normalization, reserved-device and
case-collision refusal, exact executable and repository identities with
replacement detection, and a fail-closed reparse-point policy hook. No
filesystem is touched, no NTFS behavior is exercised, and no Windows guest
exists. `WindowsPlan` now reports `ImplementedUntested` with
`filesystem_identity: true` while `native_lifecycle_evidence` stays false and
`install_service_plan()` stays `Unsupported`, matching the `MacOsAdapter`
truthfulness scheme. Sub-task `28.1.1.4` (NTFS Git behavior), Tasks `28.1.2`
and `28.1.3` (Job-Object containment, durable state, background lifecycle),
and every Gate `28.x` item requiring a genuine Windows 11 x86-64 guest remain
open.

## Implementation

- `normalize_windows_path` accepts drive-absolute, `\\?\` drive-absolute, and
  `\\?\UNC\` forms only. Drive letters uppercase, `/` becomes `\` outside the
  `\\?\` namespace, duplicate separators collapse, and `.`/`..` segments are
  refused rather than resolved so only canonical inputs normalize. Namespaces
  never collapse into each other.
- `is_reserved_device_stem` rejects `CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`,
  and `LPT1`–`LPT9` case-insensitively with or without extensions; trailing
  dots and spaces fail as alias attempts, as do `< > : " | ? *` and controls.
- `find_case_collision` folds with Unicode lowercase without collapsing stored
  identities; byte-identical duplicates are the same identity, distinct
  fold-equal spellings return their indices.
- `ExecutableIdentity` binds a canonical path to a lowercase SHA-256 digest
  and nonzero length; `check` reports `ReplacementDetected` on any mismatch.
- `RepositoryIdentity` binds an opaque volume fingerprint to a canonical root;
  any field drift reports `ReplacementDetected`.
- `refuse_reparse` admits only `ReparseKind::Absent`; symlinks, junctions,
  mount points, and unknown tags fail closed until native enumeration exists.

## Commands

```text
cargo fmt -p codingmage-platform -- --check
cargo clippy -p codingmage-platform --all-targets -- -D warnings
cargo test -p codingmage-platform --all-targets -- --test-threads=1
cargo test -p codingmage-platform --doc
python3 scripts/verification_inventory.py --write
python3 scripts/verification_inventory.py
git diff --check
```

## Results

- `cargo fmt --check`: clean.
- `cargo clippy --all-targets -- -D warnings`: no warnings.
- `cargo test --all-targets`: 18 passed, 0 failed (4 pre-existing capability
  and reference tests plus 14 new normalization, refusal, collision, identity,
  and reparse fixtures).
- `cargo test --doc`: 1 passed (reserved-device examples).
- Verification inventory regenerated: 1,109 surfaces became 1,226; every new
  surface is inventoried with exact test mappings or an explicit gap, and the
  inventory validator passes.
- One self-review correction during development: the first draft collapsed `.`
  segments in drive-absolute paths while refusing them in `\\?\` paths, and
  the new `parent_segments_are_refused_not_resolved` fixture caught it before
  any commit. Refusal is now uniform.

## Repository-Wide Suite On This Host

The full workspace suite (`cargo test --workspace --all-targets`,
single-threaded) reaches 370 passed with 2 ignored guarded soaks and
exactly 3 failures, all pre-existing environment prerequisites unrelated to
this batch and left untouched: the two guarded-startup timing tests recorded
in `docs/evidence/sprint-25-platform-faults.md` and the systemd user-manager
test above. The Python suite holds at 37 of 38 with only the known
`team.rs`/`README.md` binding freshness failure. No gate is claimed beyond the
platform-scoped and static checks listed above.

## Open Items

Native reparse-point enumeration, link-target resolution, live volume
observation, NTFS repository behavior, Job-Object containment, Windows durable
state, service lifecycle, packaging, and the genuine-guest matrix in
`Gate 28.2` all remain open under their existing prerequisites. No Windows
support beyond the validated contract is claimed anywhere, including
`docs/operations/compatibility.md`.
