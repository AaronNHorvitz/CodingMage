# ADR 0036: Render the work plan through a bounded viewport

- **Status:** Accepted
- **Date:** 2026-09-29
- **Decision owners:** CodingMage implementation

## Context

The Work plan built a widget for every matching task on each frame. The frozen native UI
qualification case includes 10,000 tasks, and the source parser already retains the complete
plan for search, dependency inspection and source locations. Rendering every row is unnecessary
when only a small scroll region is visible.

## Decision

Keep the existing source-derived `PlanIndex`, filter and coordinator-bound outcome overlay.
Flatten the filtered entries into ordered sprint, story and item display rows, then ask egui's
vertical scroll area to build only the visible range. Give each row the same interaction-height
slot and truncate its visual text within that slot. The selected task's detail and bounded source
excerpt remain below the list, so visual truncation does not replace source inspection. The
source checkbox remains disabled and distinct from the coordinator outcome.

The UI still filters and groups all matching entries in memory on each render. This decision
does not claim the frozen input, scroll, memory or installed-desktop budgets; measure those under
the specified profiles and workloads before qualification. It adds no command, execution path,
authority, dependency or licence change.
