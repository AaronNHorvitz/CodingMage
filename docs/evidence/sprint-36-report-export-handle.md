# Sprint 36 report export directory-handle increment

Task 36.3.2.2 remains open. The prior static parent check could be invalidated by a
local directory rename before the writer resolved the destination again. The report-only
writer now holds an opened Linux directory descriptor, checks its device and inode against
the requested parent, checks its current path against the target, and verifies the target
repository identity. It repeats those checks before staging, before publication and after
publication. New reports are staged as private files and published with a hard link so a
concurrent creator cannot be silently replaced when overwrite is false. Explicit overwrite
continues to refuse a non-regular leaf and uses a descriptor-relative rename of the staged
file. A mismatched published file is treated as failure, with an observed matching file
removed where possible.

Deterministic unit tests replace a requested parent link before staging and after staging,
move an opened parent into the target, replace the selected target identity, race a leaf
creation without overwrite consent and supply a static leaf symlink. They assert refusal,
absence of report bytes in the target and preservation of the competing leaf. The existing
disposable real-coordinator Reports workflow exercises ordinary outside export, overwrite,
privacy opt-in, linked-parent refusal and source-state preservation.

This is an observed-swap safeguard, not proof against every concurrent rename by a
malicious same-user process between two checks. Explicit overwrite remains a name-based
replacement after a type check. Report file I/O still runs on the UI thread and has no
exact `codingmage` command equivalent. The full section-5 state/depth catalogue, native
desktop, screen reader, frozen performance, human trials and live-provider evidence remain
open. No coordinator authority, campaign status, review outcome or source checkbox is
changed by this export.

## Verification disposition

The first focused compile failed for a missing test-only `std::fs` import; the correction
is retained in this batch. The first corrected report set passed 12/12 unit tests and
the disposable real-coordinator Reports target passed 3/3. After pre-serialization
validation was added, the final-source focused report set passed 14/14. The earlier
all-target native UI suite passed 126/126 before that narrow correction. The corrected
final-source suite also passed 126/126 across the UI's 14 test binaries, including the
real-coordinator Reports workflow, keyboard checks and offscreen software-rendered
verification (`CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=1`, shared build slot and software GL).
Strict workspace Clippy passed with warnings denied. Formatting, documentation,
architecture, no-write inventory and staged whitespace checks passed. The inventory was
regenerated after inspecting the source and generator: 1,740 public items, 825 unchanged
explicit gaps, two new report-writer surface entries, eight shifted line-derived IDs and
one heuristic applicability change. These mappings are not independent coverage claims.
Offline Cargo metadata confirmed the already pinned `nix` 0.31.3 direct dependency is MIT
licensed. The Python suite ran 42 tests: 41 passed, and only the retained CM-R01.6
source-bound evidence test failed with its same eight input-drift paths. No digest or
binding was renewed. Exact-commit independent review remains pending.
