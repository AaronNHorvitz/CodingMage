# Sprint 36 source-bound outcome command

Task 36.3.2.2 remains open. Decision 0044 adds a read-only
`campaign-outcome-report --config <absolute-file> --campaign <absolute-file>` command.
It assembles a bounded version-one JSON projection from the verified campaign authority,
authorized repository, coordinator status, blocker and final report, bound run records,
and exact-head Git changes. A second status read refuses a changed durable observation.
The command omits changed-file paths and states that UI-only admission and last-invocation
memory were not observed. Before a campaign starts, status, run evidence and change counts
are absent rather than passed or zero. An optional `--include-paths true` includes the
bounded changed-file list only on explicit request.

`report-export` assembles the same source-backed document and writes it to an absolute
destination outside the authorized repository. Its default refuses an existing leaf;
`--overwrite true` permits replacing only a regular file. It checks the held parent
directory, requested parent and repository identities around private staging and
publication. Deterministic writer tests race the parent and leaf; they do not prove
security against every possible same-user concurrent rename between checks. A killed
process may leave a private candidate in the selected outside directory.

The existing native inline schema-three report and isolated file writer are unchanged.
They do not yet invoke this schema-one public command, so exact "Show command" parity,
shared report semantics and the full section-five state/depth catalogue remain open.
This is a fake-role disposable coordinator test, not an installed desktop, live provider,
human, Orca, frozen-device performance, release or delivery qualification.

## Verification disposition

The read-only command's first disposable coordinator test passed 1/1 before file export
was added. The first `--locked` export test stopped before compilation because the new
direct `nix` dependency required an offline Cargo.lock update; that update changed only
the CLI's dependency list and no package version. A subsequent compile found a test-only
ambiguous JSON type, which was made explicit. The focused report filter then passed four
writer unit tests and the disposable coordinator test (plus one existing report-named
workflow test). These failed attempts and corrections are retained in private build output.

The first strict workspace Clippy run rejected a 114-line assembly function and a borrowed
`Option` parameter. Extracting the change projection and accepting optional references
corrected both; strict workspace Clippy then passed with warnings denied. A private-file
permission assertion was added before the full CLI all-target run. That run passed 48/48
across eight test targets; two sustained-soak tests stayed ignored because they require a
separately approved qualification profile. The export receipt was then moved ahead of
file publication so no avoidable fallible JSON encoding follows the write. The focused
report filter passed four writer unit tests, one real-coordinator export test and one
existing workflow test on that production correction; strict workspace Clippy passed.
The permission assertion was subsequently made tolerant of a more restrictive umask;
the exact final-source guarded-writer unit tests passed 4/4. A final exact-source strict
workspace Clippy check passed with warnings denied after the shared build reservation.

The inventory generator and version-one schema were reviewed before regeneration. The
current generated inventory has 1,764 public items and 825 explicit gaps: four new CLI
module functions, 14 line-derived ID moves, 14 heuristic mapping changes, no removed
surface, no applicability change and no semantic gap change. These heuristic test
mappings are an index, not a coverage claim. The Python unittest suite ran 42 tests:
41 passed, with the sole unchanged CM-R01.6 eight-input source-bound evidence freshness
failure. Its original receipt and drift remain open; no digest was renewed. Independent
review is pending, and this does not complete Task 36.3.2.2 or any human/live gate.
