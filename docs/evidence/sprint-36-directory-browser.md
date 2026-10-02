# Native directory pickers through the coordinator

This is a bounded increment of open Task 36.3.2.2 after the independent PASS
on corrective project-open commit `ef919df42469546333324ea5fc8b9124bf994d2e`.
The report was read-only, bound to that exact clean commit and found no
remaining issue in its stated project-selection scope. It does not close the
separate human, live, source-bound evidence or release gates.

Decision 0079 defines the public read-only directory-list contract. The Open,
Setup target and campaign file pickers no longer enumerate directories on the
render thread. They request a bounded version-one snapshot through the
coordinator worker. Exact-path, per-picker request and selection-generation
checks reject late or cross-slot data. The UI clears rows on navigation and
failure, shows loading/error/truncation states, offers Refresh and an exact
Show-command, and requires a successful listing before the Setup target can
be used. Symbolic links are visible but disabled; authoritative file reads and
campaign selection remain separate commands. Opening a picker starts no agent.

The CLI holds a no-follow selected directory descriptor, enumerates through
Linux procfs, rechecks the selected name, bounds scanning and output, and
reports truncation. A moving filesystem can still change an entry after the
snapshot; subsequent file operations validate independently. A missing procfs
or inaccessible directory produces an error, with no cached rows presented as
current. File names and directory paths use the existing bounded, control-safe
list labels; exact paths remain separate request data. No new dependency or
authority was introduced.

## Verification disposition

The first focused CLI target passed 3/3 after replacing an undeclared serde
derive with the existing JSON value construction. The first native library
and shell runs passed 101/101 and 16/16, respectively, before the
response-identity test and successful-listing target gate were added. Strict
workspace Clippy first identified a campaign screen method length regression;
extracting the picker panel resolved it. An initial combined CLI/UI all-target
run passed every preceding target but failed one old Work plan fixture that
expected synchronous browser rows. The corrected fixture now obtains a real
coordinator snapshot, checks Show command and then opens the selected config
through its authoritative command. That target and the native library pass on
the final source. These earlier failed runs are retained privately and are
not counted as cumulative passes.

| Check | Final-source result and limit |
| --- | --- |
| Real-process `directory_list` | 3/3 pass, including no-follow refusal, invalid arguments and bounded truncation. |
| Native UI library and Work plan | 104/104 and 7/7 pass, including request identity, malformed and unknown responses, missing coordinator and the corrected real snapshot workflow. |
| Strict workspace Clippy | Pass with `--workspace --all-targets -- -D warnings`. |
| Python unittest | 48/49; sole unchanged CM-R01.6 eight-input source-bound `input-drift` hold. No receipt or digest was renewed. |
| Formatting, docs and architecture | `cargo fmt --all -- --check`, `docs_check.py` and `check_architecture.py` pass. |
| Verification inventory | Source/schema reviewed and regenerated; no-write check passes at 1,835 items and 826 explicit gaps. |
| CLI/UI all-target cumulative rerun | 279/279 pass across 27 targets; two explicitly ignored sustained qualification cases remain unqualified. The corrected Work plan browser case passes in this run. |
| Diff whitespace and inspection | Staged whitespace passes; the 16-path staged scope and substantive source/doc diff were inspected, the inventory compared by normalized surface and applicability, no unstaged change remains, and the added-line scan found no private host path, credential assignment or private-key marker. |

The section-five state catalogue, complete Show-command coverage, detached
guided-configuration recovery read, full text externalization and native
human/device qualification remain open. This increment is builder-verified
development, not independent acceptance of the new directory-list boundary.

The inventory source and JSON-schema scan were reviewed before regeneration.
Against the parent, the normalized public API set adds the CLI listing and
four browser request/response methods, removes the synchronous browser
refresh method, and changes no existing surface applicability. The generated
inventory has 1,835 items and 826 explicit gaps, unchanged in gap count from
the parent. Line IDs and heuristic test-name suggestions move with source;
they are a work queue, not proof of test coverage. A no-write regeneration
check passes on the current source.
