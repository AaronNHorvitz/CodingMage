# Sprint 36 Task Detail Projection

Task 36.3.2.2 remains open. This increment gives Work plan an explicit source-detail path
through the sibling coordinator. `campaign-task-detail` requires the selected configuration,
campaign, exact reconciled head and one parsed item ID. It reads the configured task source
as an authorized Git blob, parses the existing plan grammar, selects exactly one item and
rechecks the head before returning. The versioned response binds campaign, repository, head,
source digest, item, source line and its digest. It includes no more than 16 KiB of UTF-8-safe
source excerpt, an explicit truncation flag and up to 100 story criteria from the same source.
Long criterion titles and additional criteria are marked as truncated.

The Work plan loads detail only on an explicit click, next to its exact **Show command**.
The UI rejects unsupported or malformed output and withholds a response after a selection,
campaign, repository or head change. Source text is rendered as inert content. A source
checkbox and story criterion are source claims; verified completion, review and tests remain
separate coordinator evidence. An absent packet or run record is never shown as a pass.

This is a local read-only feature. It does not start a provider, authorize a campaign,
qualify a real desktop or satisfy human-only acceptance. The complete state/depth catalogue,
all other command previews and localization remain open.

## Bound task run history increment

The Work plan now places a collapsed run-evidence view beside the selected source item.
It uses the existing `campaign-run-records` response, displays the exact command, and filters
on the coordinator's bound task ID. The view requires a live status and a run-record response
bound to the same head and durable status timestamp. It distinguishes an absent record from
review or test success; checkpoint failures, journal failures, omitted runs and phase limits
remain visible. Up to 20 matching runs and 16 phases per run are rendered at once, with an
explicit route to the wider Changes and reviews screen. Packet/prompt text, reviewer finding
text and full test logs are not retained in this coordinator projection and are stated as
unavailable rather than invented.

The independent review of the prior source-detail commit
`99f648bbf3e5e591485050527a3b240bd0aefe66` returned **PASS** with one Low finding:
the UI model accepted criterion IDs containing direction and terminal controls. This
increment applies the same ASCII dotted-ID validation as the head-plan response to both
the selected item and criterion IDs, and routes the displayed criterion ID through the
inert content presenter as a second boundary. The review applies to the prior commit only;
the correction and run-history increment require their own exact-source verification.

## Verification disposition

The cumulative candidate run passed CLI library 5/5, native UI all targets 100/100 under
software GL, and strict workspace Clippy. After the criterion-parent correction, final-tree
CLI library 6/6, the rebuilt-CLI disposable campaign test 1/1 and strict Clippy pass. Those
tests cover sibling-item separation, missing line, UTF-8 truncation, unrelated gate criteria,
real CLI response, unknown-item and stale-head refusal, Work plan rendering and old-response
rejection after selecting another item. Formatting,
`docs_check.py`, architecture checks, the regenerated no-write verification inventory
(1,718 items, 825 explicit gaps) and diff whitespace checks pass. Python unittest runs 42:
41 pass, while the retained CM-R01.6 source-bound freshness test fails on the same eight
drifted inputs. This work did not renew those digests. Real-provider, real desktop, Orca and
release gates remain open; the subsequent independent review is recorded above.

## Verification of the run-history and ID correction candidate

The candidate tree passed the focused UI model regression 1/1 and the
disposable real-process campaign test 1/1. The latter exercises selection, source-detail
rendering, a bound task run, its command preview, and rejection of a direction-control ID
before it can appear. The model regression rejects direction, terminal and non-ASCII-digit
IDs in both selected item and criterion fields. The full native UI all-target suite passed
100/100 under software GL, and strict workspace Clippy passed with warnings denied.
Formatting, documentation, architecture, whitespace and the regenerated inventory check
passed (1,720 public items, 825 explicit gaps; two surfaces added, none removed). Python
unittest ran 42: 41 passed and the sole CM-R01.6 source-bound freshness test failed on the
same eight retained input drifts. No digest or review record was renewed. The initial
test-only type mismatch and off-viewport pointer-disclosure failure were corrected and
their private receipts retained. This candidate still requires its own independent review;
no real desktop, Orca, human trial, provider or release result is claimed.
