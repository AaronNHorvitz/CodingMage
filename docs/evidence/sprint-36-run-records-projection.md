# Sprint 36 Campaign Run-Records Projection

Task 36.3.2.2 remains open. This increment removes the native UI's private recursive run-directory
scan and routes Changes and reports through `campaign-run-records`. Serial run references come
from the verified campaign checkpoint journal. Parallel references come from the integrity-valid
team snapshot. The coordinator checks campaign authority and the durable status before and after
reading. It refuses linked run directories and linked, oversized or malformed checkpoint/journal
files. The versioned output contains bound task IDs, bounded checkpoint and journal summaries,
absence/problem states and truncation flags, without private directory paths, prompts, source
text or provider output. A missing or malformed review/gate record remains unknown.

The interface parses a strict mirror, checks campaign, repository, head and status update time,
and clears previous evidence on failed, malformed, stale or cross-project replies. A checkpoint's
review verdict and gate IDs are withheld from the screen and exported report when its journal is
unavailable. The run projection does not create review or delivery authority. The complete state
catalogue, contextual depth and remaining Show command affordances remain open. Real Wayland/X11,
Orca, clean install, live-provider, human-trial, CM-R01.6 source-bound renewal and release gates
remain open.

The first focused campaign/state tests and CLI build passed. The first Changes integration run
passed seven of eight tests: the accepted workflow lost its journal phases because the new
projection compared the unit's campaign-worktree repository ID with the original repository ID.
The diagnostic rerun retained the exact `events.jsonl has invalid identity` result. The reader
now checks the run/task binding from verified campaign state and consistency of the internal
worktree repository ID across the verified journal. An empty journal is unavailable evidence.
The next focused Changes run passed 8/8, including a mixed-repository journal and empty journal.
Strict workspace Clippy then caught a by-value encoder parameter; the first lint result remains
failed and the encoder now borrows its input. A focused parser test filter matched zero tests and
is not counted as a pass; the full UI suite executed the parser unit test.

Before the five-pod assertion was added, the sibling CLI build, strict workspace Clippy,
runtime library tests (129/129) and UI all-target tests (86/86) passed. The earlier focused
campaign library tests (46/46) and bounded state reader test (1/1) passed. Python unittest ran
42 tests: 41 passed; the retained CM-R01.6 source-bound freshness test failed on the same eight
documented input-drift paths. No digest was refreshed and the Python suite is not green.
Formatting, documentation, architecture, regenerated inventory validation and diff checks passed.
The private logs retain both failed iterations, the focused repair and the cumulative result.
The deterministic inventory reports 1,696 surfaces and 825 explicit gaps; it is candidate
mapping evidence, not proof that every declaration has a test. Independent review remains pending.

The existing five-pod parallel CLI workflow was extended to read the projection after serialized
integration; it passed 1/1 with five bound checkpoints and journaled review observations. On the
additional test source, strict workspace Clippy and CLI library tests (4/4) passed. Python again
ran 42 tests: 41 passed and only the same CM-R01.6 eight-path input drift failed. The original
failed Python receipt remains retained, and no evidence digest was renewed.

This is a deterministic fake-provider result, not live-model or independent acceptance.
