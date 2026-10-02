# Native command palette increment

Task 36.3.2.2 remains open. Decision 0087 adds a fixed `Ctrl+K` palette for
all nine existing destinations and the read-only diagnosis refresh. Search
matches first-party labels, not repository or model content. The diagnosis
entry uses the same bound `doctor --config` arguments as the top-bar action;
its in-place Show-command expands without running it. With no coordinator,
the entry is disabled while navigation remains available. Escape closes the
palette without changing the selection.

The offline 1024×640 shell test reaches Work plan by arrow/Enter, filters to
the unavailable diagnosis action, confirms Enter cannot issue a request and
closes with Escape. A disposable repository test expands the exact doctor
preview, activates via Enter and observes a newer diagnosis from the real
coordinator. A unit test checks fixed-list filtering. A minimum-window
offscreen software-rendering test checks visible navigation, search and
command preview. The private frame was inspected at 1024×640 on llvmpipe
(CPU/Vulkan software adapter); it showed the full palette and always-visible
status bar. This is a rendering check, not installed Wayland/X11 or Orca
qualification.

The first focused shell run failed because a hint text was not an AccessKit
label for the search field. A visible label and `labelled_by` association
corrected that finding; the rerun passed 2/2. The first screenshot test
failed because both the background navigation and palette correctly expose
the same Help destination. The assertion now checks that both nodes exist;
the rerun passed 1/1. These exploratory failures are retained privately.

Diff inspection found an activation error before the cumulative checkpoint:
Enter on the focused Show-command disclosure could also activate the
palette's selected action. Keyboard selection now responds to Enter and
arrow keys only when the search field held focus before the input frame;
Enter on the disclosure only opens or closes it. The first focused run of
that correction failed 2/2: egui released single-line search focus while
processing Enter, so checking focus afterward suppressed activation. The
corrected code remembers the search widget ID and tests its prior-frame
focus. The real-coordinator regression checks that no new diagnosis is
observed after disclosure Enter, then focuses search and observes a newer
diagnosis after its Enter. Final-source focused shell tests pass 2/2, and
the software-rendering/AccessKit test passes 1/1. The first broad test run
began before this correction and passed 214/214 across 14 targets; it is
kept as pre-correction evidence.

## Verification disposition

Final-source native UI all-target passed 214/214 across 14 targets under
the shared build slot, one Cargo job, one test thread and labelled software
rendering. Final-source strict workspace Clippy with warnings denied passes. The
inventory generator and schema discovery were reviewed before regeneration:
1,857 surfaces, 826 explicit gaps, four added palette Rust surfaces, no
removed surface or existing applicability/mapping change. The full Python
suite ran 50 tests: 49 passed, with the sole unchanged CM-R01.6 eight-input
source-bound evidence failure. No bound receipt or digest was renewed.
Formatting, documentation, architecture and whitespace checks pass.
The exact-commit independent review of
`2a02350268acd61ce31dbb09b0bba3744a8e8c41` returned PASS with no
findings. The reviewer independently ran native UI 214/214, strict workspace
Clippy, formatting, documentation, architecture and inventory checks, and
observed the same CM-R01.6 Python failure. This is bounded review of the
palette increment only. The full
state/depth/Show-command catalogue, private Setup recovery command boundary,
human desktop trials, live-provider qualification and release gates remain
open.
