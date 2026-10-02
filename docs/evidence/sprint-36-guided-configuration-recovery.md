# Guided configuration recovery through the coordinator read boundary

Task 36.3.2.2 remains open. Decision 0082 moves the guided configuration
recovery check off the native render thread. It retains Decision 0076's
detached, one-shot public writer and private terminal record. A recovery
request binds the existing intent path, SHA-256, request ID, selection
generation and current UI binding. The bounded worker checks the private
result and exact named bytes, then calls the public `project-open` command
under a deadline. The returned closed-schema snapshot must name the selected
path, carry the exact written-byte digest and decode to the submitted
configuration. A failed public read, missing result, changed bytes, malformed
snapshot or stale response cannot report a successful recovery.

The first worker result does not clear the private record. After the UI
accepts it, a second worker request rechecks the successful receipt and
destination before cleanup. This preserves recovery if the window closes
before accepting the result. Explicit notice clearing remains a distinct
owner action after inspecting the destination; it cannot turn an unknown or
failed write into success. A queue refusal retains the private intent. A
failed automatic cleanup withdraws the provisional project selection and
retains the notice. Neither recovery request replays the write or admits a
campaign.

The disposable native Setup target covers a real guided write, conflicting
policy and no-overwrite refusal, a helper that survives window close, a
missing task source during initial recovery, destination replacement after
publication, changed selection, manual notice clearing and the exact digest
on a real `project-open` snapshot. The tests wait for asynchronous responses
and verify that a refused recovery leaves the private intent. The first
focused run failed three old synchronous-click assertions. Those fixtures
were changed to wait for a recorded response; the corrected focused target
passed 15/15. After the separate accepted-result cleanup correction and
digest test, the focused target passed 16/16. These are local tests, not an
independent review or installed desktop qualification.

The final native UI all-target suite passed 204/204 across 14 targets under
the shared build reservation, one Cargo job, one test thread and labelled
software rendering. Strict workspace Clippy with warnings denied passed on
the final source. Formatting, documentation, architecture and whitespace
checks passed. The inventory source and schema were reviewed before
regeneration: 1,837 to 1,842 items, 826 explicit gaps before and after, five
new public recovery surfaces and no changed existing applicability or gap
tuples. Test-name hints in that inventory are not proof of coverage.

The full Python suite ran 49 tests: 48 passed and only the unchanged
CM-R01.6 source-bound evidence check failed on its eight recorded drifting
inputs. The original failed receipts and binding remain intact; this batch
does not renew their hashes or claim the external qualified-human review.

The public campaign-selection correction from Decisions 0080 and 0081 was
separately reviewed at exact commit
`faab58e0c9d3065053292881a484a8608e1dd1ca`; its bounded independent
report returned PASS with no findings. That result does not review this
guided configuration batch. CM-R01.6's eight-input source-bound freshness
failure, the rest of the section 5 state/depth and Show-command catalogue,
qualified-human security and accessibility checks, real-provider trials and
release gates remain open.

Private intent discovery during app construction and creation when the owner
submits the form are still local UI-thread operations. This increment covers
the potentially blocking outcome, destination and public project read on the
recovery path. The guided campaign write receipt and the complete action
command catalogue remain separate open work in Task 36.3.2.2.
