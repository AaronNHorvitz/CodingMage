# Sprint 36 campaign status recovery increment

Task 36.3.2.2 remains open. The Campaign screen has an explicit **Refresh campaign status**
action beside its durable status observation. It sends only `campaign-status` for the selected
configuration and campaign through the existing bounded backend worker. Its adjacent **Show
command** disclosure presents the exact argument vector. If the executable or a representable
path is unavailable, the action is disabled. Opening the disclosure does not submit work.

The action reads coordinator state; it does not start, resume, cancel or authorize a campaign.
The screen labels retained values as the last observation while a refresh is in flight. After
a failed or malformed response it keeps the prior value stale, says that current progress
cannot be confirmed, and offers this explicit read-only recovery. A successful bound response
clears the failure. Missing sibling-coordinator guidance now describes the actual installation
path; Setup has no executable picker.

The disposable-repository integration test runs the real coordinator through a fake-provider
campaign, injects a foreign campaign status response, checks that the old observation is
retained as stale, opens the exact command by keyboard, and activates the refresh. It verifies
that a new selected-campaign observation clears the error without changing UI generation.
The first draft of that test incorrectly compared the entire status object: elapsed time is
sampled anew on every read. The corrected assertion checks stable campaign, head and state
identity plus a new observation time. The failed draft result is retained in the private
handoff; it was a test expectation error, not a coordinator regression.

This is one state and recovery slice. Other section 5 states and action previews, desktop
accessibility, human trials, provider qualification and release gates remain open.

## Verification disposition

The focused real-process recovery test passed 1/1 after the test expectation correction.
The final candidate passed all 101 native UI all-target tests with software rendering and
strict workspace Clippy with warnings denied. Formatting, documentation, architecture and
diff whitespace checks passed. The deterministic verification inventory reports 1,720 items
and 825 explicit gaps, unchanged in count; its broad test mappings are heuristic and do not
establish qualification. Python unittest ran 42 tests: 41 passed, and the sole failure was the
existing CM-R01.6 source-bound evidence freshness check on the same eight drifted inputs.
No evidence digest was renewed. Independent review of this batch remains pending.
