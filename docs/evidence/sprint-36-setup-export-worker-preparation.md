# Setup export worker preparation

Decision 0086 moves creation and synchronization of a new private Setup
export intent to the bounded native worker. The UI freezes the selected
configuration, diagnosed repository, source, destination and shown public
command. It accepts a closed, request-bound preparation response only while
the same selection remains current, then dispatches the existing detached
`setup-export-copy` helper. A failure, malformed response or changed selection
cannot report export success; a persisted unresolved private intent is
retained for recovery. The public coordinator command and export receipt
checks are unchanged.

The disposable real-process test holds the worker before export submission,
checks that the UI returns without creating the intent, then releases the
worker and verifies the destination matches the source. A malformed response
case confirms that no public export is launched while the private notice
remains for inspection. A separate case waits for the worker to persist the
intent without advancing the UI, changes campaign selection before accepting
the queued preparation response, then checks that the late response leaves
the private intent in place without launching the public command. The worker
unit checks exact response digest, unknown-field rejection and an oversized
input without an intent file. Existing Setup recovery and window-close cases
were updated to advance the now asynchronous interface while waiting for
their public helper markers.

## Verification disposition

The initial focused Setup run failed two existing marker waits because those
tests did not advance the interface after the new asynchronous preparation;
the waits were corrected and Setup passed 19/19 before the final stale test.
The focused worker unit passed 1/1. The first strict workspace Clippy run found
a test-only `u64` to `usize` cast; it was replaced by a checked conversion and
strict Clippy then passed. These initial failures remain disclosed.

The first stale-selection test held the worker before preparation, so
generation cancellation correctly prevented the intent from being written;
its assertion that a retained intent must exist timed out. The corrected case
changes selection after the durable intent appears but before response
acceptance, directly exercising the retained-intent path. The corrected
focused stale case passed 1/1, and the final-source Setup suite passed 20/20.

The native UI all-target run passed 209/209 across 14 targets using the shared
build reservation, one Cargo job, one test thread and labelled software
rendering. It compiled before the final test-only stale case was added; no
production behaviour changed afterward. Final-source strict workspace Clippy
with warnings denied and the focused Setup suite passed. Formatting,
documentation, architecture and whitespace checks passed after formatting a
test-only assertion that the first final check flagged. The verification
inventory generator and schema discovery were reviewed before regeneration:
1,853 items, 826 explicit gaps, exactly three new Rust surfaces, no removed
surface and no existing applicability or gap-set change. The full Python suite
ran 50 cases: 49 passed; only the unchanged CM-R01.6 eight-input source-bound
`input-drift` failed. Its original receipts and binding were not renewed.

No live provider, installed desktop, GPU, human accessibility or runtime-model
qualification is claimed. Task 36.3.2.2 remains unchecked: private recovery
controls lack a public command equivalent and the full state/depth/Show-command
catalogue is still open. Independent read-only exact-commit review of
`761adec6c02a035686e6bd3ff6d6b29c9d1d21e6` returned PASS with no
findings. The reviewer independently ran native UI all-target 210/210,
focused Setup 20/20, the worker unit, strict Clippy and static checks from
a clean source. This bounded verdict does not qualify the open catalogue,
desktop, human, live-provider or release gates.
