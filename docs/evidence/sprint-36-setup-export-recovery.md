# Setup export recovery on the bounded worker

Decision 0084 moves reopening, outcome/destination inspection and notice
cleanup for the detached Setup export into the existing bounded native
worker. The UI applies only matching request, generation and selection
responses. A helper completion starts a fresh verification and a verified
observation starts separate cleanup. The UI reports success only after that
cleanup completes; explicit inspected clearing reports only local notice
removal. An unknown, malformed, stale or replaced destination stays
unconfirmed. The export remains an exact public coordinator command, while
private recovery controls have no public command equivalent.

Disposable tests exercise normal export, malformed receipts, replacement
after a public receipt, window close and campaign reselection, helper survival
and manual clearing while a helper is running. A focused worker regression
checks that verified cleanup rechecks destination bytes and that a wrong
request cannot clear the notice.

## Verification disposition

The first focused Setup run passed 15/16. Its one failure used the old
same-frame notice-clear assertion after cleanup became asynchronous; the
corrected focused suite passed 16/16. The first strict Clippy run found
semicolon, borrowed-response and function-size issues in the new code; those
were corrected and the final strict workspace Clippy with warnings denied
passed. These failed checks remain part of the private work record.

The native UI all-target suite passed 205/205 across 14 targets under the
shared build reservation, one Cargo job, one test thread and labelled
software rendering. That run compiled before the final loading-label and
Clippy-only refactor; the final-source Setup integration target then passed
16/16, and strict workspace Clippy compiled every final target. The new
worker unit regression passed in the all-target library suite. No GPU,
runtime-model qualification or new dependency was involved.

The verification inventory source and schema were inspected before
regeneration. It now has 1,850 items and 826 explicit gaps, seven added
surfaces and no removed surfaces compared with the parent. Existing
applicability categories did not change; the new private recovery
decoders/checks have explicit malformed, unknown-field and boundary
applicability. Its 11 focused mutation/freshness tests passed. Inventory
test-name suggestions are not acceptance evidence.

Final formatting, documentation, architecture and whitespace checks
passed. The full Python suite ran 50 cases: 49 passed and only the
unchanged CM-R01.6 eight-input source-bound `input-drift` check failed.
Original failed receipts, package and binding are retained; no digest or
human approval was renewed. This increment awaits fresh exact-commit
independent review.

Task 36.3.2.2 remains unchecked. New export intent preparation still runs
on the interface thread; private recovery controls lack a public
`codingmage` command equivalent, and the complete section-five state,
depth and Show-command catalogue remains open. Human desktop/accessibility
checks, separately admitted live-provider qualification, CM-R01.6
qualified-human package review, full-product review, licence and release
remain separate.
