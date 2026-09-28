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
control and presses Enter, proving keyboard access and expansion at the same viewport; its
focused rerun passed 1/1. Both failed runs are retained privately. The final three-row
minimum/default/high-DPI/large window check passed 1/1. The minimum and high-DPI frames were
visually inspected, and the pixel digests above refer to that final layout. The final-source
cumulative batch passed the sibling CLI build, strict workspace Clippy and native UI all-target
tests (91/91). Python unittest ran 42 tests: 41 passed and the sole failure remained the
CM-R01.6 source-bound evidence binding on its same eight drifted inputs. Formatting,
documentation, architecture, verification inventory (1,702 surfaces and 825 explicit gaps) and
diff whitespace checks passed. No source-bound evidence digest was renewed. This evidence does
not qualify a live provider, a desktop environment, Orca, human usability, the frozen performance
budgets, packaging or release.
