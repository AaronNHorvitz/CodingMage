# Sprint 36 Help Message Catalogue Increment

Task 36.3.2.4 remains open. The native Help screen's orientation, recovery guidance, keyboard
guidance, glossary and source-notice labels now read an embedded versioned English catalogue.
The catalogue is parsed with the existing TOML and Serde dependencies and refuses an
unsupported schema, a missing or unknown key, duplicate keys and empty messages. The shipped
interface continues to show English and offers no incomplete language or expertise mode.

Synthetic test variants expand every migrated message by at least 40% and prefix right-to-left
script text. A display-less Help render checks labelled controls at the minimum 1024×640
window and 200% scaling with right-aligned layout. This is a local layout stress test, not a
human screen-reader result or complete bidirectional navigation qualification.

The remaining UI screens, dynamic Help and support-bundle text, dates, numbers, durations,
exact packaged third-party licences and full right-to-left behavior are not yet externalized
or qualified. The frozen specification and device budgets are unchanged.

## Focused verification

The catalogue validation and 40% expansion unit cases pass 2/2. The display-less Help preview
passes 1/1 at 1024×640 logical pixels and 200% scaling; it checks that expanded and
right-aligned synthetic labels and navigation controls remain present with accessibility
bounds. An initial owned test harness compile error used an obsolete `eframe::App::update`
method; it was corrected to the pinned `ui` method and the failed output remains in the
private session receipt. A subsequent `--exact` filter selected zero tests and was not
counted; the unfiltered focused rerun selected and passed the intended case.

At this source checkpoint, the software-rendered native UI all-target suite passed 104/104
and strict workspace Clippy passed. Formatting, documentation, architecture, diff whitespace
and no-write inventory checks passed. The reviewed inventory generator and schema were
unchanged; regeneration found 1,724 items and 825 explicit gaps. Its crate-wide, capped
test mappings changed mechanically when the new tests were added and do not establish
coverage or qualification. Python unittest ran 42 tests: 41 passed and the sole failure
was the retained CM-R01.6 source-bound freshness check on the same eight input drifts.
No evidence digest was renewed. Independent review of this increment is pending.
