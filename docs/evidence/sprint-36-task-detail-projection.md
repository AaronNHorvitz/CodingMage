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
drifted inputs. This work did not renew those digests. The independent reviewer,
live-provider, real desktop, Orca and release gates remain open.
