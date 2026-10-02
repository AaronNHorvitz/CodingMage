# Campaign specification read correction

The independent read-only review of `6a50b5dfc3f1bdbb05f5e2c996a98e8e86b2eab3`
found that native campaign selection could follow a file replaced after a
picker snapshot. That review's High finding remains open until the native
selection command and response binding are implemented and independently
re-reviewed.

This first, backend-only correction changes the shared campaign specification
loader. It holds a no-follow descriptor, checks ordinary-file identity before
and after the bounded read, and parses those held bytes. A deterministic
ordinary-file-to-link replacement is refused. This is builder verification,
not independent acceptance or live campaign qualification.

## Backend checkpoint verification

The deterministic replacement case passed 1/1 and the complete
`codingmage-campaign` all-target suite passed 48/48 on this source. Strict
workspace Clippy with `-D warnings`, `cargo fmt --all -- --check`,
`docs_check.py`, `check_architecture.py` and diff whitespace passed. The full
Python suite ran 49 cases; 48 passed and the sole failure remains the same
CM-R01.6 eight-input source-bound `input-drift` hold. No binding, digest,
package or failed receipt was renewed. Original failure details remain in
private test output. The inventory generator was reviewed before `--write`;
its 1,835 items and 826 explicit gaps retain the same normalized public API,
applicability and gap sets. Changed source locations and heuristic test-name
suggestions are not accepted coverage proof.

CM-R01.6 source-bound evidence renewal, human and live gates remain open.
