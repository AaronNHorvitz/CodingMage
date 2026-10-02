# Guided configuration parent correction

The public `setup-write-config` boundary now admits the native form's default
new workspace directory as one checked, owner-private direct child of an
existing external parent. It does so only after parsing the submitted candidate
and binding its target path to the explicit selected repository. The guarded
writer rechecks the resulting destination before publication. A malformed
candidate and a missing destination parent inside the repository create no
directory. After a later failure, an empty private parent or root may remain
for inspection; the command does not claim rollback of an uncertain effect.

This is a backend prerequisite for the still-open native configuration action.
The independent review of the preceding ADR 0074 commit found no source issue
but remained inconclusive because its isolated Cargo commands could not obtain
a valid shared-slot run. This correction is a separate commit and needs its own
exact-source independent review. Final-source CLI Setup tests passed 14/14,
and strict CLI all-target Clippy with warnings denied passed. The first Clippy
pass found an owned 103-line function; extracting prospective-parent validation
resolved it before the final run. Cargo formatting and architecture checks
passed. Human, live-provider and release qualification remain open.

The staged-tree verification inventory remains 1,808 public items with 825
explicit gaps. It has no added or removed normalized surface, no applicability
change, and 12 heuristic mapping changes from the new malformed-input test;
these mappings do not establish coverage. Documentation, architecture and
formatting checks passed on the isolated staged tree. The full Python gate is
deferred to the native integration batch while that uncommitted source is in
progress; its existing CM-R01.6 eight-input source-bound hold is unchanged.
