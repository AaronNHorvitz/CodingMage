# Sprint 33 director priority admission

Task 33.1.1.2 implements a pure coordinator-side admission check for the
existing source-bound director proposal. A valid proposal still cannot
execute work. Its full ready-task permutation and original mission criteria
are revalidated before the mission's `Ordering` decision evaluator can return
`PermittedChoice`; every other outcome is a typed hold. A declared dependency
between simultaneously ready tasks is refused. The policy decision is
content-free and carries the entire campaign path scope to avoid silently
narrowing what later pod proposals may touch.

The fresh bounded independent re-review of Decision 0089's exact commit
`2753c969c725408e470122da4b7b123c2f217969` returned PASS, Findings None,
and explicitly closed the prior Medium arbitrary failure-code finding. It
does not review this Decision 0090 increment or close any human/live gate.

The first focused compile of this batch found two test fixture bindings
shadowing the `context` helper; corrected. A later new negative-case test
had the same test-only shadowing issue; corrected. The final focused director
filter passes 7/7. The inventory source and schema were inspected before
regeneration. The generated inventory moves from 1,882 to 1,884 surfaces:
one public function and one stable error code. No logical surface was removed,
no existing applicability category changed, and all 826 explicit gaps remain.
The generator's first-eight sorted test mapping shifts on 156 old logical
surfaces; those shifts do not prove new coverage. Final-source all-target
tests under the shared heavy slot passed: campaign 54/54, runtime 133/133,
and native UI 214/214 across 14 targets. The native tests used labelled
software rendering, one Cargo job and one test thread. Format, documentation,
architecture and diff-whitespace checks also passed before the task ledger
update. Strict workspace Clippy with warnings denied passed. The full Python
suite ran 50 tests: 49 passed and only the unchanged CM-R01.6 source-bound
freshness test failed with its same eight input drifts. Its original failed
receipts, package and binding remain untouched. Final format, documentation,
architecture, inventory no-write and staged whitespace checks passed after
the ledger update. The exact staged source, decision and task diff was
inspected; the inventory's logical keys and applicability were compared to
the parent. An added-line scan excluding the generated inventory found zero
private host path, credential-assignment or private-key markers in 428 lines.
No manifest or lockfile changed.

The exact-commit bounded independent review of
`91d87229e6469bbe4e83c6030f383e799995563e` returned PASS with no
findings. Its reviewer independently passed the focused director and
campaign targets, strict Clippy and static inventory checks; a queued Python
run never started and is not claimed as an independent pass. The report
identified a future Task 33.1.1.3 consumer requirement to bind durable
decision identity to the selected domain and alternative as well as the
proposal, because retry and answer state keys on decision ID. That future
work remains open. This bounded PASS does not renew CM-R01.6 or qualify
human/live work.

Director invocation, durable decision application and runtime role
qualification remain Task 33.1.1.3 and later work. CM-R01.6 still needs the
separately authorized qualified-human source-bound review before package
renewal. No source-bound digest, failed receipt or package was renewed here.
