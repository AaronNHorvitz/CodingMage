# Setup export cleanup repository binding

The bounded independent review of commit
`d6c49243e721b5de441cb78815c4539e83c00809` returned `FINDINGS` with one
High finding: manual cleanup could remove an export intent and terminal result
for repository A after the same configuration pathname was reopened for
repository B. The report and prior successful local checks remain retained;
this correction does not change that verdict.

Decision 0085 adds a current project/diagnosis gate to both cleanup paths,
disables the manual control on a mismatch, and carries the observed repository
identity and configuration to the worker for an exact intent recheck. The
disposable integration case creates an A export with a retained malformed
terminal receipt, replaces the configuration at its pathname with B's real
configuration, reopens it, attempts the disabled manual control, and confirms
that both A records remain. The focused worker unit also refuses a foreign
repository or configuration while preserving both records. A matching
observation can still clear an inspected notice.

## Verification disposition

The first focused Setup integration compile failed because the new test did
not import the AccessKit node trait. After that test-only import correction,
the new disposable case passed 1/1. Native UI all-target tests passed 206/206
across 14 targets with the shared build reservation, one Cargo job, one test
thread and labelled software rendering, including Setup 17/17. That all-target
run preceded a test-only extraction of repeated worker assertions after the
first strict Clippy run reported its 100-line function limit. Final-source
strict workspace Clippy with warnings denied and the focused worker unit
test 1/1 then passed. No production behaviour changed after the all-target
run.

The verification inventory generator and schema discovery were inspected
before regeneration. It has 1,850 items and 826 explicit gaps, unchanged from
the parent. Only line-position identifiers and test-name mappings moved; no
surface, applicability category or gap-set changed. Its no-write check and
the full Python suite's inventory tests pass. Documentation, architecture,
formatting and whitespace checks pass.

The full Python suite ran 50 cases: 49 passed and only CM-R01.6's unchanged
eight-input source-bound `input-drift` failed. The original failed receipts,
package and binding remain intact; no digest or human approval was renewed.
No GPU, runtime-model qualification or new dependency was involved.
Independent re-review of the correction is pending. Task 36.3.2.2 and all
human, live-provider, licence, release and whole-product gates remain open.
