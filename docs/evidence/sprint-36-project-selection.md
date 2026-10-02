# Native project selection through a public read command

This is a bounded increment of open Task 36.3.2.2 on the source tree after
`c47243a87250ab67f33aaecfb877760808372181`. Decision 0077 records the
command and native client contract. The Open and recent-configuration actions
now submit `codingmage project-open --config <absolute-file>` to the bounded
worker. Opening remains read-only for the target repository and starts no
agent. The only local write after a successful response is the established
private recent-list entry.

The version-one response contains the validated configuration, its selected
path and exact-byte digest, and a strict task-plan result. Unavailable,
invalid and output-too-large plans are distinct from a configuration failure.
An invalid plan retains the parser's stable content-free reason code, while
an unknown reason code is rejected as a contract failure.
The CLI bounds source input at 8 MiB, caps its JSON output plus the entry-point
newline at 8 MiB, refuses a task-source path resolving outside the selected
target and checks the
configuration bytes against the loader result. The UI checks the response
schema, exact selected path, digest grammar and parsed-plan identity. A
selection change cancels the queued request and rejects any late response.
The Open control shows the exact submitted command while the request is
pending. A missing sibling command or malformed result leaves no opened
project and has an explicit recovery message. A later doctor observation with
a different task-source digest clears the old plan and gives a reopen action;
the old rows are not presented as current work. A doctor target fingerprint
that differs from the opened configuration is rejected as an authority
mismatch and also clears the plan.

Disposable real-process checks cover unchanged target/configuration bytes,
an invalid, missing, symbolic- or hard-linked and oversized task source,
malformed arguments,
an oversized parsed JSON projection, missing coordinator, malformed and
unsupported response schemas, cross-project identity, a changed real task
source observed by doctor, a forged cross-target doctor observation, a delayed
worker response and a Show-command preview. The first CLI test compilation failed
because the pinned SHA-2 result does not implement `LowerHex`; it was fixed
before the focused rerun. The first UI all-target run passed its core and
campaign/changes targets but stopped with one test that assumed synchronous
Open before selecting a campaign; the corrected focused case passes. A first
shell rerun also found a test querying the Open preview on the Work plan
screen; navigating to Setup corrected that fixture. A broad UI run found a
readiness fixture that expected downstream campaign controls when its hidden
executable prevented project selection; the corrected case checks that those
controls remain unavailable. These failures remain in
private test receipts and are not presented as final-source passes.

The Setup directory picker still lists files on the render thread and guided
configuration recovery still loads the written file in-process while
checking its held receipt. Those are separate open parts of Task 36.3.2.2;
this record does not close the section-five state catalogue, all in-context
depth or all Show-command controls. Task 36.3.2.4's locale/RTL and the
installed desktop, Orca, human trial, real-provider, independent and release
gates remain open. CM-R01.6's source-bound package review remains external;
no input digest, receipt or binding was renewed here.

## Verification disposition

The focused real-process `project_open` target passed 3/3, including the
output-bound case. The corrected combining-mark campaign-action case and
corrected Open-preview shell case each passed 1/1. Verification checks and
their exact result are recorded below. The cumulative run preceded
the final target-fingerprint check and lint refactor; focused final-source
tests and strict workspace Clippy cover that bounded change.

| Check | Result and limit |
| --- | --- |
| CLI and native UI all targets | 270/270 across 26 targets on the candidate before the final target-fingerprint check and method extraction; final-source `project_open` 3/3 and shell 16/16 pass. |
| Strict workspace Clippy | Pass, `cargo clippy --workspace --all-targets -- -D warnings` on the final source. Initial size and test-helper lints were corrected. |
| `cargo fmt --all -- --check` | Pass on the final source. |
| Documentation and architecture checks | `docs_check.py` and `check_architecture.py` pass on the final source. |
| Verification inventory | Source/schema reviewed and regenerated; no-write check passes at 1,830 items and 825 explicit gaps. Focused Python inventory suite passes 10/10 after the final heuristic pins. |
| Python unittest suite | 48/49. Sole failure is the unchanged CM-R01.6 source-bound `input-drift` hold across eight named inputs; original failed receipts and bindings remain. |
| `git diff --check` | Pass; staged inspection remains before commit. |

The inventory comparison against the parent has three added normalized
public surfaces and none removed. Only the `open_project` boundary category
changes on an existing facet; seven unrelated neighbouring facets are pinned
against context-heuristic drift. Normalized explicit gaps remain 825 with no
additions or removals. Sixty-five line-derived IDs and 28 capped test-name
suggestion sets move; suggestions do not prove coverage.
