# Sprint 36 Work plan viewport increment

Task 36.3.2.2 and the frozen native UI performance qualification remain open. Work plan now
keeps the source parser's complete task set for filtering, dependencies and selected-item detail,
while egui builds widgets for only the visible scroll rows. Sprint and story headings are rows in
the same order as their source-derived items. A fixed row height and single-line visual
truncation keep scroll positions stable; the selected item's detail remains inspectable.

A disposable repository fixture writes 10,000 open sub-tasks, commits the task source, opens it
through the real coordinator and displays the plan at 1024×640 logical pixels in a display-less
egui harness. The test asserts the parsed 10,002 source items, fewer than 100 visible task-row
accessibility labels, absence of a distant offscreen row, search and selection of that row, and
unchanged task-source bytes. The fixture is synthetic; it does not admit or start a campaign.
The bounded widget count is local rendering evidence, not a measured frame-time or
assistive-technology result.

The list still scans matching items and constructs display metadata on each render. No frozen
profile's input, live-scroll, idle, memory or 10,000-task latency budget has been qualified.
Installed Wayland/X11, Orca, scaling and human trials remain external checks. Decision 0036
records the rendering choice.

## Verification disposition

The focused real-source Work plan target passed 6/6, including the 10,000-task case, after the
fixed-height truncation and minimum-size correction. The first cumulative run passed the UI
library's 48 tests but failed one campaign test (7/8): its pointer click targeted a Show command
control below the visible outer scroll region after the list grew taller. The failure reproduced;
focus and Enter activated the same control. The fixture now exercises that keyboard action, and
the corrected native UI all-target suite passed 118/118 across 14 targets.

Strict workspace Clippy first rejected the enlarged Work plan function at 106 lines against its
100-line limit. The viewport renderer moved to a private helper; strict workspace Clippy with
warnings denied then passed, and the exact final source passed the native UI suite 118/118 again.
Formatting, documentation, architecture and diff whitespace checks passed. The regenerated
verification inventory has 1,738 public items and 825 explicit gaps, with only one shifted
line-derived identifier; no public surface, stable error code, heuristic test mapping or gap
changed. Python unittest ran 42 tests: 41 passed, and only the retained CM-R01.6
eight-input source-bound evidence drift failed. No evidence digest was renewed.
Independent exact-commit review is pending. No human, live-provider, release or delivery
gate changes.
