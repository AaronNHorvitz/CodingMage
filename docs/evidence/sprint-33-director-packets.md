# Sprint 33 director packet contract

Task 33.1.1.1 defines a version-one director planning input and inert priority
proposal under Decision 0088. The source binding includes the exact campaign
authority, mission charter, canonical task-source digest, durable scheduler digest and
generation, and observed campaign head. Sprint progress counts literal source
checkboxes; it is not verified completion. Proposal persistence uses the
existing private integrity document and reload checks fresh source state.
No runtime director is invoked and no packet mutates canonical task state.

The focused campaign and runtime tests exercise a valid packet over parsed
canonical tasks, exact mission criteria and scheduler generation, then store
and reload an integrity-bound proposal. They refuse an invented ready task,
duplicate criterion, malformed failure code, impossible remaining budget,
wrong source-checkbox count, unknown JSON field, changed campaign head and
tampered private document. No test invokes a model or changes task state.

The first focused source pass was 2/2. The first strict workspace Clippy pass
passed, but the architecture check found a forbidden campaign-to-state edge.
Moving only persistence into the runtime owner corrected the dependency;
Cargo.lock returned to its prior bytes and the architecture check now passes.
After that correction, focused campaign and runtime filters passed with the
new director cases. The first corrected strict Clippy pass found one new
`format!`-collection allocation, corrected by reusing the runtime's hex
encoder; strict workspace Clippy now passes.

The verification inventory generator and schema discovery were reviewed
before regeneration. It now lists 1,881 surfaces, up from 1,857, with 826
explicit gaps unchanged. There are 24 added inventory records in the new
director files, representing 23 distinct path/kind/name keys because two
methods share the public name `verify`; no old logical surface was removed.
Existing applicability categories did not change. The generator's bounded
first-eight crate-level test mapping changes on 515 old surfaces because new
test names enter the sorted lists; this is a mapping limit of the inventory,
not new coverage or a change to the open gaps.

## Verification disposition

Final-source campaign all-target passed 50/50 and runtime all-target passed
133/133. The linked native UI all-target suite passed 214/214 across 14
targets with one Cargo job, one test thread and labelled software rendering.
Strict workspace Clippy with warnings denied, formatting, documentation,
architecture, inventory no-write and diff whitespace checks pass. The full
Python suite ran 50 tests: 49 passed, and only the unchanged CM-R01.6
`test_multi_agent_evidence_binding_is_current` failed with the same eight
source-bound input drifts in the original evidence package. No package,
receipt or digest was renewed. The exact ten-file staged diff was inspected,
including both new Rust modules, the ledger, decision and evidence text; the
large inventory-only text delta is the generated mapping shift described
above. A scan of 995 added non-inventory lines found no private host path,
credential assignment or private-key marker. No Cargo manifest or lockfile
change remains in the batch.
Task 33.1.1.2's outcome/domain policy validation, role invocation and
qualification, Task 36.3.2.2's Setup recovery command equivalence, and
qualified-human/live/release gates remain open.
