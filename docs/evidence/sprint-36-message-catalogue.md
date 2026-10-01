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
drift. No digest was renewed. At the original checkpoint, independent exact-commit review
was pending; its later result is recorded below.

The inventory extractor and schema were reviewed before regeneration. It still has 1,751
items and 825 explicit gaps, with no semantic public surface or stable error-code change.
Fourteen line-derived public item IDs moved and 107 capped, crate-wide heuristic test
mappings changed after the new Reports test. These mappings are an index, not evidence
that the mapped tests cover each listed public item.

Independent read-only review of exact commit
`b3360ebce8700f1c42033b3386c48697b83547df` returned a bounded **PASS** with one Low
finding: the previous sentence counted twelve relocated line-derived IDs instead of
fourteen. Comparing the committed base and candidate inventories by semantic facet
confirmed nine relocations in `reports_screen.rs` and five in `messages.rs`; this
correction changes only the evidence count. The reviewer did not complete a separate
Python suite because its shared build reservation did not become available; the
builder's 41/42 receipt remains the stated Python evidence. The review does not
qualify installed accessibility, full RTL, human trials, live providers or release.

## Campaign static-copy increment

The Campaign destination now reads 77 additional static headings, authority and outcome
labels, status and mission-state guidance, browser controls and recovery actions from the
version-one English catalogue. The no-repository state offers direct Setup and Help actions;
the no-campaign state offers Setup. Browsing and these navigation actions still start no
coordinator or agent. The validated catalogue remains the single source for the migrated
copy, and its existing unit check expands every key by at least 40% with a right-to-left
stress variant. A display-less Campaign empty-state render checks the expanded labels,
AccessKit bounds and Setup navigation at 1024×640 logical pixels and 200% scale.

Dynamic status and mission sentences, dates, durations, numbers and some other Campaign
copy still come from Rust formatting. Other destinations and full bidirectional keyboard
navigation also remain open under Task 36.3.2.4. This synthetic preview does not qualify
installed Orca, desktop use, the frozen device budgets, human trials or live providers.

The preceding report-assembly commit `8bd04ba10fc6e2428d78aaf297ec9b7e52f4910a`
received an independent **INCONCLUSIVE** verdict: the reviewer found no static code issue,
but could not acquire the shared build reservation to run the required Cargo checks. The
builder's prior passing checks do not replace those independent checks. The review report
remains private and unchanged; a later exact-commit review must resolve this limit.

The first exact-filter focused run selected zero tests and was not counted. The corrected
focused Campaign preview selected and passed 1/1. The exact-source software-GL native UI
all-target suite passed 137/137 across 14 targets, including the real-coordinator disposable
campaign, reports and recovery cases. Strict workspace Clippy with warnings denied passed.
The Python suite ran 42 tests: 41 passed and only the unchanged CM-R01.6 source-bound
freshness check failed on the same eight input drifts. No evidence digest was renewed.

The inventory generator and schema were reviewed before regeneration. It still lists 1,759
items and 825 explicit gaps: 23 line-derived IDs moved with the code, 112 shared entries
changed by line/context or capped heuristic test mapping, and no semantic public item,
schema or gap was added or removed. Those mappings do not prove coverage. Final-source
formatting, documentation, architecture, no-write inventory and staged diff whitespace checks
passed. The exact seven-file staged diff was inspected; its 680 added lines contained no
private-root path, credential assignment or private-key marker. Independent exact-commit
review was pending at that checkpoint. No task, acceptance criterion or gate was closed by
this increment.

The independent read-only review of exact commit
`923b14f2b1f806dfaefbd7b059e2153395d7087e` later returned a bounded **PASS**
with no findings. It independently rebuilt and passed 137/137 native UI all-target tests,
including the inherited report-assembly increment. Reviewer Clippy remained unrun after
the shared build reservation did not become available; the builder's strict workspace
Clippy receipt is separate. The verdict does not qualify the full native product or any
human, live or release gate.

## Campaign dynamic-template increment

Decision 0043 adds declared named fields to the existing version-one English catalogue.
The parser refuses missing, unknown, repeated and malformed fields, including fields
inserted into a static message. Field order can change without changing the meaning;
backend values are inserted as literal text and remain inert in the native renderer.

The selected Campaign screen now obtains dynamic campaign identity, execution shape,
binding drift, observation, active work, coordinator blocker, final commit, hold and
utilization rows, and mission status/authority summaries from validated templates.
The values still come from the same bound coordinator and campaign records. This batch
adds no language selector, coordinator command, authority or dependency. Other screens,
remaining Campaign/Help dynamic copy, locale-specific numbers, dates and durations,
full right-to-left behavior and installed screen-reader checks remain open.

The template mutation and literal-value tests exercise missing/forged/repeated fields,
unbalanced delimiters, reordered fields and brace-like backend text. The existing
synthetic 40% expansion applies to the new keys. Final exact-source check results for
this increment are recorded below after the cumulative suite completes.

The inventory generator and version-one schema were reviewed before regeneration.
The source-consistent inventory has 1,760 items and 825 explicit gaps: the new formatter
adds one public surface, five line-derived IDs move, and 437 common entries receive
crate-wide capped heuristic test-map changes after the named tests were added. No public
surface was removed and no applicability category or gap was added or closed. These
heuristic mappings are an index, not a claim that each mapped test covers its entry.

The first focused Rust attempt reached compilation and failed because the mission mode
`String` was supplied where a borrowed field value was required. The call site now
borrows that value. The failed compiler output is retained in the private build record.
The corrected focused catalogue suite passed 4/4; the cumulative checks follow below.

The first cumulative native UI run passed 139/139 across 14 targets on the source after
that correction. Strict workspace Clippy then rejected two new number arguments passed
by value and a mission-rendering function two lines above the workspace limit. The
arguments are now borrowed and the mission authority label is selected in a small
helper. That failed Clippy result is retained privately. The Python suite had not
started because the script stopped at Clippy; the corrected-source checks below
supersede this failed attempt.

On the corrected final source, `cargo clippy --locked --workspace --all-targets -- -D warnings`
passed. The software-rendered native UI all-target suite passed 139/139 across 14 targets,
including the four catalogue unit tests and the disposable coordinator workflows.
Formatting, documentation, architecture, no-write inventory and diff whitespace checks
passed. Python unittest ran 42 tests: 41 passed and the sole failure was the unchanged
CM-R01.6 eight-input source-bound evidence freshness check. No digest or binding was
renewed. The tests used one Cargo build job, one Rust test thread, software GL and the
shared heavy-check reservation; they do not qualify an installed desktop, Orca, device
budgets, human trials or a live provider. Task 36.3.2.4 and all higher acceptance/gates
remain open. Independent review of this exact increment is pending.

## Work plan orientation and observation copy

Decision 0051 moves Work plan first-run and source guidance, filter labels,
coordinator-status observations and final-report recovery copy to the validated
version-one English catalogue. Exact named fields carry the source path, counts,
freshness, age, head freshness and recovery action. The existing command builders,
coordinator authority and report visibility rules are unchanged. Runtime values
remain inert text. The shipping interface remains English with no language or
expertise selector.

The catalogue mutation unit rejects missing, repeated and forged Work plan
observation fields; it permits reordering while preserving literal brace-like and
script-like values. The synthetic 40% expansion and right-aligned right-to-left
preview checks the Work plan empty-state labels, AccessKit bounds and Help
navigation at 1024×640 logical pixels and 200% scale. Both focused cases passed
1/1. This preview covers a first-run recovery path, not a loaded plan, installed
screen reader or complete bidirectional keyboard traversal.

Work plan row labels, source detail and task evidence, other screens' remaining
text, locale-specific number/date/duration formats, exact packaged third-party
licences and full right-to-left behavior remain open under Task 36.3.2.4.
The independent review of the preceding exact commit
`4d9272176c5c2fa71c104003161a664870084431` returned **PASS** with no source
findings. Its report also lists a prerequisite build and two broader tests run
without the required build-slot wrapper. Those commands are not counted as
resource-compliant independent heavy checks; its focused regression and strict
Clippy did use the slot. The review does not qualify this new increment or any
human, live or release gate.

The software-GL native UI all-target suite passed 151/151 across 14 targets
under the shared heavy-check reservation, including the Work plan catalogue
units and disposable Campaign, Changes, Reports, verification and Work plan
workflows. Strict workspace Clippy with warnings denied passed after the UI
suite. Python unittest ran 42 tests: 41 passed and the sole failure was the
unchanged CM-R01.6 source-bound freshness test on its same eight input drifts.
No evidence digest was renewed. Formatting, documentation, architecture,
no-write inventory and diff-whitespace checks passed on the final source.
The exact staged diff and additions were inspected for scope and private host
or credential material before commit. Independent review of this increment
remains pending; installed accessibility, full RTL, live and human gates stay
open.

The unchanged inventory generator and schema were inspected before regeneration.
The source-consistent inventory lists 1,770 items and 825 explicit gaps. Against
the preceding checkpoint, no semantic public item was added or removed; seven
line-derived IDs moved and 113 shared entries changed only in capped crate-wide
heuristic test mappings after the new test was added. Applicability and gap counts
did not change. These mappings are an index, not proof of coverage for an item.

Independent read-only review of exact commit
`cdc609c3e1216dc2678c75cb2f653d78efa308d1` returned a bounded **PASS**
without findings. It reran the named-field and empty-state preview units plus
strict workspace Clippy through the required shared slot. The reviewer did not
rerun the builder's 151-test native UI suite or Python suite. The verdict
does not close Task 36.3.2.4 or any installed accessibility, human, live or
release gate.

## Loaded Work plan row and parsed-detail copy

Decision 0052 extends the existing catalogue to Work plan filters, sprint and
story headers, source-checkbox guidance, outcome badge labels, parsed row
identity/readiness and item-detail fields. Named templates require the exact
source identifier, title, checkbox, readiness, line and digest values. The UI
still renders source text through the bounded inert content path. Existing
English wording, parser, coordinator commands and outcome authority remain
unchanged. The public plan crate's English label helpers stay for compatibility;
the native screen now selects catalogue text for those labels.

A focused mutation unit rejects missing, repeated and forged row identity
fields and checks that brace- and script-like source text remains literal;
the focused run passed 1/1.
The loaded-plan synthetic preview selects an item and checks its row and
parsed detail in 40%-expanded and right-aligned variants at 1024×640 logical
pixels and 200% scale; its focused run passed 1/1. These are local layout and
accessibility-tree probes, not complete RTL keyboard navigation or installed
screen-reader evidence.
Coordinator-provided detail, source excerpts, run evidence, locale number and
date formats, exact packaged third-party licences, full bidirectional behavior
and human trials remain open. Final-source check results and inventory
disposition are recorded below.

The first software-GL native UI all-target suite passed 153/153 across 14 targets
under the shared heavy-check reservation, including the loaded-plan preview,
row-template mutation unit and disposable coordinator workflows. Strict
workspace Clippy with warnings denied passed on that source. Staged diff
inspection then found the unknown-outcome template was not reusing its
separately catalogued badge value. The template now requires the badge field,
and the loaded-plan preview checks it. English output is unchanged. The first
run remains prior-source evidence. The corrected final-source software-GL suite
also passed 153/153 across 14 targets, and strict workspace Clippy passed.
Final-source Python unittest ran 42 tests: 41 passed and the sole failure was
the unchanged CM-R01.6 eight-input source-bound freshness check. No digest was
renewed. Formatting, documentation, architecture, no-write inventory and
whitespace checks passed on the corrected source. The exact staged diff and
added lines were inspected for scope and private-root/credential material
before commit. This increment awaits independent exact-SHA review; no
installed, human, live or release gate is closed.

The inventory generator and version-one schema were inspected before
regeneration. It still lists 1,770 semantic public items and 825 explicit
gaps. Seven line-derived IDs moved; the `failure_box` entry lost a heuristic
boundary applicability tag because its nearby source context changed. That
does not change the function or establish a new coverage claim. One capped
heuristic test mapping changed with that tag. No semantic item was added or
removed, and no gap count changed.
