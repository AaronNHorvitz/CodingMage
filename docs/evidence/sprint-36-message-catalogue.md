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

## Independent review and next catalogue slice

Independent read-only review of exact commit
`ea5bf8a27b0407e4ec16ade0482c6be3908eaf3b` returned a narrow **PASS**. It confirmed
the earlier Decision 0016 Setup-picker correction and the catalogue boundary. Its one Low
finding was that the specification baseline's later summary still called pseudo-locale and
right-to-left tests absent. The baseline now distinguishes the existing Help-only synthetic
test from the still missing application-wide coverage and full bidirectional behavior.

The next implementation slice moves Help's manual-diagnostics headings, form labels, command
disclosure label and pending/failure guidance, plus Settings' appearance labels and explanatory
text, into the same bundled English catalogue. Settings continues to change only this window's
palette. It has no language or expertise selector, and Help's dynamic coordinator outcomes
still come from Rust formatting and remain to be externalized. Synthetic Settings labels are
checked at 1024×640 logical pixels and 200% scale with right alignment and AccessKit radio
roles; the Help preview also checks the manual-diagnostics labels. These are limited local
stress checks, not complete RTL navigation or human accessibility results.

The focused Settings preview passed 1/1 after an initial test query matched both a radio and
the current-palette label; the failed attempt is retained privately. Extended catalogue
validation and synthetic expansion passed 2/2, and the expanded Help preview passed 1/1.
Inventory regeneration is deterministic at 1,724 items and 825 explicit gaps; the changed
source lines move 45 line-based item IDs, while common-item applicability and capped test
mappings are unchanged. For this second slice, the software-rendered native UI all-target
suite passed 105/105 and strict workspace Clippy passed. Formatting, documentation,
architecture, no-write inventory and diff whitespace checks passed. Python unittest ran 42
tests: 41 passed and the sole failure was the retained CM-R01.6 source-bound freshness check
on the same eight input drifts. No evidence digest was renewed. Fresh independent review of
exact commit `5b643b01558204bde1dfa1b4cfe9b464fa4e9b5e` returned a narrow PASS with no
findings. It did not qualify human, live or full-product gates.

The next open slice adds authored English keys for the Workspace selected-campaign summary. Its
labels distinguish current versus retained status, recorded counters versus recent history, and
known attention versus an unavailable observation. The existing catalogue parser and synthetic
40% expansion unit test cover the new keys; real-process rendering passed in the focused
campaign suite, the final native UI all-target suite passed 105/105, and strict workspace Clippy
passed. Python retains only CM-R01.6's eight-input freshness failure. No new language selector or
broader right-to-left claim is made.

## Reports static text increment

The Reports destination now uses the same bundled English catalogue for its static headings,
source-freshness warning, outcome and blocker labels, export form, privacy guidance and
recovery instruction. The catalogue retains exact-key and nonempty validation; no runtime
locale or language selector is introduced. The English text and coordinator authority are
unchanged. A synthetic 40% expansion and right-aligned right-to-left preview checks the empty
Reports state plus outcome and blocker summary labels at 1024×640 logical pixels and 200% scale.
This tests accessible label presence and bounds, not complete screen-reader navigation or
bidirectional layout. Dynamic export results, observation wording, coordinator codes, report
document content, dates and numbers still need externalization. The export command and
render-thread report assembly/JSON preview remain separate open Task 36.3.2.2 work.

The first focused preview failed because a partial AccessKit label query matched both the
expanded label and existing report content; the corrected test queries the full catalogue
message. The corrected focused test passed 1/1. The software-GL native UI all-target suite
passed 132/132 across 14 targets, including the disposable real-coordinator Reports and
setup-to-outcome workflows. Strict workspace Clippy (`-D warnings`), formatting,
documentation, architecture, inventory and diff-whitespace checks passed. Python unittest
passed 41/42; the sole failure is the unchanged CM-R01.6 eight-input source-bound evidence
drift. No digest was renewed. This increment has not received independent exact-commit review.

The inventory extractor and schema were reviewed before regeneration. It still has 1,751
items and 825 explicit gaps, with no semantic public surface or stable error-code change.
Twelve line-derived public item IDs moved and 107 capped, crate-wide heuristic test
mappings changed after the new Reports test. These mappings are an index, not evidence
that the mapped tests cover each listed public item.
