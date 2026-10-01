# Sprint 36 report mount-boundary correction

Independent exact-commit review of `0ec26baa39bd730e551227ee6dfbaeec2b233578`
returned `Verdict: FINDINGS`: a bind-mounted alias of the target could bypass the
path-only outside-repository guard. The report is retained in the private review
directory. Decision 0045 applies a conservative same-mount rule to both the CLI
and native helper writers. It leaves the existing held-directory, canonical-path,
identity, private-staging, overwrite and post-publication checks in place.

The deterministic test creates temporary directories and runs two bounded child
processes in private Linux user and mount namespaces. One binds the repository
root, the other a nested repository directory, to an outside alias. Each child
checks that normal and explicit-overwrite exports refuse before creating a report
leaf. The fixture requires user/mount namespaces and the local `mount` utility;
it does not grant host mount privileges or establish installed-desktop behavior.

The same-mount restriction can refuse safe destinations on another filesystem.
Concurrent same-user mount or rename changes between checks and publication remain
a limitation, as in the earlier export evidence. The native Reports screen still
uses a different report schema and helper; exact public Show-command parity and
Task 36.3.2.2 remain open. No live, human, release or delivery claim follows.

## Verification disposition

The focused CLI and native-helper bind-mount tests each passed 1/1 after checking
that both child assertions actually ran. Cumulative checks are recorded below.

The focused CLI report filter passed seven tests: five writer units, the disposable
real-coordinator outcome/export fixture and one existing workflow test. The native
report-export filter passed eight tests, including the new namespace fixture and
the existing path, race and overwrite regressions. The first strict workspace
Clippy run found one needless borrow in a new CLI test; after correcting that
test-only style issue, strict workspace Clippy passed with warnings denied.

On the corrected tree, the software-rendered native UI all-target suite passed
140/140 across 14 test targets, including six Reports integration tests.
The exact final-source CLI bind-mount unit passed again after the style correction.
The Python unittest suite ran 42 tests: 41 passed, with the sole unchanged
CM-R01.6 eight-input source-bound evidence freshness failure; no digest was
renewed. `cargo fmt --all -- --check`, `scripts/docs_check.py`,
`scripts/check_architecture.py`, no-write verification inventory and staged
whitespace checks are the final light gates. The inventory remains at 1,764
items/825 explicit gaps: no new or removed public surface, four line-derived ID
moves and 18 heuristic mapping changes, with no applicability or gap change.
These mappings are an index rather than an acceptance claim. The independent
read-only re-review of exact commit
`4cf186e2e1b2725f3ef64dada9a2e5c91770ee62` returned `Verdict: PASS` with no
findings after inspecting the correction and independently running focused CLI,
native, strict lint and light checks. Its scope is the mount-boundary correction;
it does not qualify a later native action, full product, human or live gate.
