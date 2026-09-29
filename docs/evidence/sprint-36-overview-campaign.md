# Sprint 36 Workspace campaign summary increment

Task 36.3.2.2 remains open. The Workspace summary uses the selected campaign and its existing
bound `campaign-status` observation. It displays the coordinator state, separately recorded
completed, blocked, deferred and human-decision counts, and a reported campaign blocker code or
an attention notice derived from nonzero counts. It labels the observation's freshness and age;
a retained value after a failed or overdue refresh is marked stale, including an earlier absent
status. A nonzero coordinator hold count also triggers attention even when the task counters
are zero. An absent status remains distinct from a campaign that has not started. The only action
on the summary navigates to the Campaign screen, where the existing exact-command refresh and
recovery controls live.

The summary does not invent a recent-outcome timeline, objective capture or a final review
verdict. The coordinator's current projection does not provide a chronological Workspace
history. The summary does not authorize or start a campaign, edit task source, grant policy, or
make a new backend request.

## Verification disposition

Real-process campaign tests for a selected unstarted campaign, a paused campaign with one
completed unit, a partial campaign with a blocked task, and a foreign-status stale observation
pass in the focused seven-test campaign suite. The first run failed three assertions: the
summary was below the visible page, its outcome sentence doubled punctuation and the test
mistook a zero blocked-task count for no campaign blocker. The summary was moved to the top,
punctuation corrected, and the test now expects the coordinator's reported attention; the
failed result remains in the private session receipt. The first cumulative run passed 105/105
native UI tests and strict workspace Clippy; Python passed 41/42 with only the existing CM-R01.6
eight-input drift. Two final fail-closed corrections were then made for stale retained absence
and a reported hold with zero task counters; the corrected focused campaign suite passed 7/7.
The bundled English catalogue has 19 additional required keys and the existing synthetic
expansion test applies to them. The final-tree software-rendered native UI all-target suite passed
105/105 and strict workspace Clippy passed. The Python suite passed 41/42, with only the same
CM-R01.6 eight-input source-bound freshness drift; no digest was renewed. Formatting,
documentation, architecture, deterministic inventory (1,724 items/825 gaps) and diff whitespace
checks passed. Five inventory IDs shifted with source lines, without a semantic surface or test
mapping change. Real desktop/Orca, human trials, live providers, CM-R01.6 source-bound renewal
and independent exact-commit review remain open.
