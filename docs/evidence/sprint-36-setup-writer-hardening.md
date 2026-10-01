# Sprint 36 guided Setup writer correction

## Scope

The existing guided Setup still performs local file writes, which remains an open
command-boundary and render-thread gap against the accepted native UI specification.
This increment changes the safety of those writes while the public sibling-command
migration is outstanding.

Configuration and campaign specifications are still built with their existing
typed contracts. Their exact candidate bytes pass `load_config` and
`CampaignSpec::load`, respectively, before publication. Authorization records and
document exports now use the same descriptor-bound, repository-checked writer as
report exports. It refuses a parent linked into the target repository, preserves
pre-existing candidate files, uses a new candidate and atomic no-overwrite
publication, and rejects a changed parent. Authorization and export destinations
must have an existing readable parent. The configuration form checks its
destination and scratch/state roots against the canonical repository before
creating roots, then checks the roots again. The Setup screen supplies the
selected repository path to export instead of a lexical prefix check. The
authorization writer refuses text above its declared 1 MiB record limit before
creating a file.

The correction changes neither coordinator execution authority nor provider
qualification. It does not close Task 36.3.2.2: Setup still needs public CLI
commands, an asynchronous worker and exact Show-command disclosures. Full
desktop accessibility, human trials and live-provider gates remain open.

## Verification

The corrected Setup unit cases pass 3/3, including linked repository parents for
configuration, campaign, authorization and export, a scratch root under a linked
parent, oversized authorization text and preservation of a pre-existing candidate
file. The reused report writer's seven fault-injection cases pass, covering linked
and moved parents, changed repository identity, leaf symlinks and concurrent leaf
creation. The disposable real-coordinator Setup integration target passes 3/3:
guided configuration, authorization, campaign selection, export and diagnosis
failure all retain their existing observed behavior. The first focused run failed
one fixture because the selected authorization-file parent was absent; the fixture
now creates that directory before invoking the writer. The original failure is
retained privately, not counted as a final-source pass.

The verification-inventory generator and its version-1 artifact contract were
inspected before regeneration; neither source nor schema changed. The regenerated
inventory has 1,771 items and 825 explicit gaps. Its one new public surface is
the shared document writer; `export_bytes` and `export_copy` gain the bound
repository argument and boundary applicability. Twelve line-derived IDs move.
Candidate test mappings are an index, not proof of coverage. Cumulative UI,
Clippy, Python and light-gate results below apply to this local increment.

The software-GL native UI all-target suite passed 163/163 across 14 targets,
including the disposable real-coordinator Setup workflow and report writer
fault-injection tests. The last edit after that suite was a test-only checked
conversion for the 1 MiB fixture; the final-source Setup unit and integration
targets were rerun separately and passed 3/3 each. Strict workspace Clippy with
warnings denied passes after that correction. The initial Clippy failure is
retained in the private session log. Formatting, documentation, architecture,
no-write inventory and diff-whitespace checks pass. The full Python unittest
suite ran 43 tests:
42 passed; the sole failure is the pre-existing CM-R01.6 source-bound freshness
binding on the same eight unrelated inputs. No evidence digest was renewed.
Independent exact-commit review is pending. None of these checks qualifies an
installed desktop, screen reader, live provider or required qualified-human gate.
