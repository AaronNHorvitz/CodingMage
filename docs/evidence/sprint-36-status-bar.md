# Sprint 36 always-visible native status projection

This is an increment under open Sub-task 36.3.2.2. The native shell now projects bound,
read-only coordinator observations in its bottom panel on every destination. The panel shows
repository and campaign identity, campaign state, mission involvement, distinct identified
active pod IDs and the coordinator's last checkpoint timestamp. The complete section 5
catalogue, contextual depth and all command equivalents remain open.

`campaign-status` schema 5 has campaign state, active tasks with optional pod IDs and
`updated_at_ms`; `campaign-mission-status` schema 1 has the selected charter's involvement.
The consumer first checks response envelope and payload identities. A repository label is
paired with an abbreviated repository ID, and the full path and ID are available on hover.
The active pod count is the number of distinct nonempty pod IDs reported by the coordinator;
zero identified pods is not a claim that no work exists. For no selected campaign, not-yet-
observed or failed status, the count is unknown. A coordinator-confirmed `null` campaign status
means not started and has zero active pods. Retained status or mission values are visibly
marked stale after a failed refresh or age expiry. A charter's involvement is labelled as last
observed; its revoked or expired flag is shown without granting authority.

The coordinator status schema has no current-gate field. Every panel therefore says “Current
gate: not reported by coordinator.” It does not infer a gate from a task phase, a retained run
record or this repository's tests. Backend checkpoint time is shown exactly as milliseconds
since 1970 UTC; localized human-friendly formatting remains open under Sub-task 36.3.2.4.
The status bar performs no new subprocess, filesystem read or control action. See
[Decision 0028](../decisions/0028-native-status-bar-projection.md).

## Verification disposition

The first focused test was red 0/1 at the absent repository label in the previous bar. The first
implementation attempt had a compile-time closure-scope error; it was corrected, with the
failed run retained privately. The corrected never-started, foreign-payload/stale/recovery and
empty-shell focused cases each passed 1/1. The foreign-payload case also checks that two active
tasks naming the same pod produce one identified pod and no invented current gate. The
software-rendered window-size test then passed 1/1 at 1024×640, 1100×720, 200% scaling and
1920×1080. The first minimum screenshot showed an awkward wrap of the last-update label;
the corrected layout gives identity, state and timing their own rows. The corrected minimum
and 200% screenshots were visually inspected. The 1024×640, 100% screenshot has pixel digest
`00089ae01087ce353dcea8597bc44d53d492603429e34798cc2abb46e788d360`; the 1100×720,
200% screenshot has pixel digest
`cf3e079beefc867cb7fd53fd2a47401e235cc520dccdc58905922da0040c3a71`. These are
offscreen wgpu/llvmpipe CPU renderings; raw fixture-path screenshots and their manifest are
retained privately to avoid publishing machine-local paths. The first cumulative attempt passed
the sibling CLI build, then strict Clippy found a similar local name and an unnecessary option
closure in the status-bar source; both were corrected without changing projection behavior.
The second attempt found that the expanded foreign-payload test exceeded the workspace's
function-length limit; its pod assertion was extracted into a focused helper. Both original
Clippy diagnostics are retained privately. A later strict Clippy run found one inefficient
test-string assignment in the extracted helper; that was corrected with `clone_into`, with
the diagnostic retained. The fourth attempt passed the CLI build and strict Clippy, then the
existing real preflight integration test failed 3/4: its offscreen click on “Show command” did
not expand the preview after the status bar grew. The optional status message was moved into the
timing row to restore vertical space without hiding required fields. A focused rerun still
failed with a click; the control appeared in the accessibility tree. The test now focuses that
control and presses Enter, proving programmatically focused expansion at the same viewport; its
focused rerun passed 1/1. This did not prove Tab traversal or visibility. Both failed runs are
retained privately. The final three-row
minimum/default/high-DPI/large window check passed 1/1. The minimum and high-DPI frames were
visually inspected, and the pixel digests above refer to that final layout. The final-source
cumulative batch passed the sibling CLI build, strict workspace Clippy and native UI all-target
tests (91/91). Python unittest ran 42 tests: 41 passed and the sole failure remained the
CM-R01.6 source-bound evidence binding on its same eight drifted inputs. Formatting,
documentation, architecture, verification inventory (1,702 surfaces and 825 explicit gaps) and
diff whitespace checks passed. No source-bound evidence digest was renewed. This evidence does
not qualify a live provider, a desktop environment, Orca, human usability, the frozen performance
budgets, packaging or release.

## Independent review and correction candidate

The exact-commit independent review of `9965b055891f2a3a92d1986df9751b8f74f4a2a9`
returned **INCONCLUSIVE** with two applicable findings. First, the status and mission schemas
carry string codes, so an identity-bound but unknown code could appear as an ordinary state or
involvement setting. The corrected UI now recognizes the current producer's closed phase and
involvement code sets and labels any other code as an **unknown coordinator value**, showing only
a bounded inert preview of that code. The status bar, Campaign and report views use the same
campaign-state label; the bar and Campaign mission view use the same involvement label. A bound
synthetic `future_state`/`future_mode` regression passed 1/1. The raw coordinator codes remain in
the observation and exported machine-readable report; this display correction grants no authority.

Second, the reviewer found that direct focus in the prior readiness test bypassed keyboard
traversal. The corrected test selects Campaign by shortcut, traverses with Tab at the 1024×640
logical minimum, checks that the preflight control owns focus and has accessibility bounds, then
opens its exact command with Enter. That focused test initially passed 1/1 while a private frame
showed the focused control still offscreen. The shared command-preview widget now scrolls a
focused header into view. The corrected focused test passed 1/1, and the private offscreen
llvmpipe frame was visually inspected with the preflight control visible. Its raw-pixel digest is
`ff8d439effff4c20ec6d3bd80df39b716fb1ea71cb36024154a64f8856cbbfe7`;
the prior offscreen frame and compiler error remain private. This proves the tested software
viewport, not real desktop or Orca accessibility. The final-source cumulative batch passed the
sibling CLI build, strict workspace Clippy and native UI all-target tests (91/91), including the
bound unknown-code regression, preflight keyboard traversal and resource/recovery tests. Python
unittest ran 42 tests: 41 passed and the sole failure remained CM-R01.6's same eight source-bound
input drifts; no digest was renewed. Formatting, docs, architecture, verification inventory
(1,703 surfaces, 825 explicit gaps) and diff checks passed. Earlier compiler and Clippy failures
remain in private receipts. The exact-commit independent re-review of
`210201bcc669e3d17f2f6e3875127d67c6d90794` found both prior defects corrected by static
inspection and no new applicable finding, but returned **INCONCLUSIVE**: the reviewer could not
acquire the required shared build slot to rerun the focused Rust tests and strict Clippy. The
reviewer independently passed documentation, architecture, formatting, inventory and the
source-bound provenance check and verified the supplied screenshot digest. This scoped review
does not close Task 36.3.2.2 or human, desktop, live-provider, package or release gates.
