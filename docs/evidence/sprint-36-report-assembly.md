# Native report assembly and preview boundary

Task 36.3.2.2 remains open. The Reports screen previously assembled an outcome document on
every render and serialized the inline preview each time it was visible. Decision 0042 moves
assembly and preview serialization to a CPU-only worker separate from coordinator commands and
the isolated report writer. The interface clones one request-time observation snapshot when the
selection, source observation, source revision, admission, terminal invocation, freshness or
repository-path choice changes. It displays only a result matching that key. Export remains
unavailable while the current report is being prepared and refuses a direct request against an
older key. A source response received in the same render poll invalidates the previous result.

The inline preview is limited to 128 KiB of serialized data and explicitly says when truncated.
The guarded export receives the full immutable report. The report worker's one-request queue and
nonblocking response channel preserve a finite shutdown path for this CPU-only task. The
coordinator-control worker remains separate; the report worker never opens a file or runs a
coordinator command. The existing export-helper filesystem containment and its documented
rename and timeout limits remain in force.

The focused disposable Reports suite checks export privacy, path and overwrite refusal,
source-failure freshness, and new stale-snapshot export refusal. A serializer unit test compares
a small preview to full export bytes and checks that a large preview is capped while the full
document remains intact. The native UI guide describes the loading and export behavior.

## Verification disposition

The first focused Reports run passed 5/5 but compiled before the new case was added. The
source-revision rebinding and stale-export case passed 1/1, and the bounded serializer unit
case passed 1/1. The final corrected-source software-GL native UI all-target suite passed
136/136 across 14 targets, including Reports 6/6 and the complete disposable fake-provider
workflow. Strict workspace Clippy first found four style issues in the worker/render code;
the second run found five redundant test closures. Both failures are retained privately.
The corrected exact-source strict workspace Clippy run passed.

Formatting, documentation, architecture, no-write verification inventory and diff-whitespace
checks passed. The inventory generator and schema were reviewed before regeneration: 1,759
items and the same 825 explicit gaps. It lists six new UI source entries, no removed item or
closed gap, 65 line-derived ID movements and five heuristic applicability/mapping changes.
Those mappings are an index, not a coverage claim. The Python unittest suite ran 42 tests:
41 passed, and only the unchanged CM-R01.6 source-bound evidence-freshness test failed on its
same eight input drifts. No package binding or digest was renewed. The added-line privacy scan
found no private host path or credential marker. Independent review of this new source is pending.

## Remaining boundaries

The public `codingmage` report-export command equivalent, full section-5 screen/action/depth
catalogue, full text localization and RTL, frozen performance budgets, installed desktop and
Orca checks, human trials, live-provider qualification, independent whole-product review and
release authority remain open. This local worker change grants none of those outcomes.
