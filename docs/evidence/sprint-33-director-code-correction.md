# Sprint 33 director failure-code correction

The independent bounded review of
`d68a25fe4faf971261cbcde632506215bdb4b380` returned PASS with a Medium
finding: the prior string validation accepted arbitrary lowercase codes such
as `private_token_fragment`. Decision 0089 closes that field over existing
typed campaign task terminal reasons, excluding success. The director packet
remains inert and has no runtime provider producer in this increment.

The focused campaign director filter passes 4/4 after correcting one test-only
compile error in its first attempt. The new test accepts and round-trips all
15 documented non-success terminal reasons and refuses unknown valid-looking
codes, prose and `merged`. The original review report is retained privately
and unchanged.

The inventory source/schema was inspected before regeneration. The generated
inventory now has 1,882 surfaces, one more than the reviewed parent: the new
public `DirectorFailureCode` enum. No logical surface was removed, no existing
applicability category changed, and all 826 explicit gaps remain. The
generator's bounded, sorted test mapping shifted on 159 existing logical
surfaces when the new test name entered campaign-crate lists. This mapping
change does not claim new coverage for those surfaces.

Final-source campaign all-target passed 51/51; runtime all-target passed
133/133; linked native UI all-target passed 214/214 across 14 targets with
one Cargo job, one test thread and labelled software rendering. Strict
workspace Clippy with warnings denied, formatting, documentation and
architecture checks passed. The full Python suite ran 50 tests: 49 passed,
and only the unchanged CM-R01.6 source-bound freshness test failed with its
same eight input drifts. The original receipt, package and digest were not
renewed. The inventory no-write check passed with 1,882 surfaces and 826
explicit gaps. The exact eight-file staged diff was inspected; a scan of 200
added non-inventory lines found no private host path, credential assignment
or private-key marker. No manifest or lockfile changed. Exact-commit
independent re-review remains pending.
