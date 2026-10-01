# Public Setup command boundary evidence

## First public write: authorization record

Task 36.3.2.2 remains open. Decision 0064 introduces the public
`setup-write-authorization` command as the first bounded Setup write. Its
arguments identify an existing configuration, the exact repository ID observed
by `doctor`, and a requested output outside that repository. A stale or
cross-project identity is refused before stdin is read. The held repository
identity and Git HEAD are revalidated at the final publication boundary. The exact record is
sent by stdin, limited to 1 MiB, validated as nonblank UTF-8 without NUL, and
never put in arguments, stdout, stderr or a preview. The result is a
content-minimized JSON receipt
with the bound repository identity and SHA-256 digest. The reviewed CLI report
writer supplies no-overwrite by default, explicit atomic replacement, and
the previously recorded uncertainty behavior.

The final-source real-process disposable test covers an initial private-mode
file, digest receipt, no-overwrite refusal, explicit replacement,
blank/malformed/NUL/oversized input, stale repository identity, output inside
the repository and invalid overwrite grammar. Its code and test names are in
`crates/codingmage-cli/tests/setup_authorization.rs`.
Both real-process tests pass (2/2). CLI unit tests pass (20/20), including
side-effect-free exact help and the reviewed report writer cases. Existing
core repository revalidation tests pass (2/2) for a changed Git HEAD and a
replaced target directory. Strict workspace Clippy with warnings denied,
`cargo fmt --all -- --check`, documentation checks, architecture checks,
no-write verification inventory and `git diff --check` pass. The Python suite
passes 42/43 tests with the sole unchanged CM-R01.6 eight-input source-bound
freshness failure; the package and failed receipt were not renewed. A broad
CLI all-target run completed the unit (20/20), mission (16/16), preflight
(1/1) and repair (2/2) targets before it was stopped during a longer campaign
target because the post-input authority revalidation source edit made that
run non-final. It is a partial regression check, not a full-suite pass. The
first focused compile failed because the installed Rust toolchain does not
support the attempted `Result::is_err_or`, and the pinned SHA-2 type does not implement
`LowerHex`. Both were corrected before rerunning; the failed output remains
in private session evidence. This is not a behavioral failure or a test pass.

Configuration, campaign and document-export writes still run in the UI
process, and this authorization action has not yet been routed through a
bounded native worker. Exact Show-command and stdin disclosure remain open.
Fake or disposable tests do not establish human, installed-desktop or live
qualification. No old review report or source-bound package digest is
changed. The generated inventory has 1,774 items and 825 explicit gaps;
the gap set is unchanged. One public Rust surface was added, and line IDs
and heuristic test mappings shifted. Those mappings are not verified coverage.

## Independent finding and local correction

Independent review of `d77a67414af09b5669c1483c21e442d5d8fe0454` returned
`FINDINGS` with one High item. A disposable real-process test delayed candidate
sync, changed Git HEAD before publication and observed a success receipt. The
report is retained privately and has not been edited. The initial results
above remain historical results for that source, not a pass for the correction.

Decision 0065 adds a caller-supplied authority check after candidate byte and
destination validation, directly before create-only linking or overwrite
exchange. A second check after publication suppresses a success receipt if the
repository changes during publication; because bytes may already be visible,
that path returns `codingmage.cli.uncertain_write` and retains the private
stage for reconciliation. A deterministic test changes fixture Git HEAD at
each boundary for both create and overwrite. Before publication it requires
`stale_observation` with no new or replaced public leaf; after publication it
requires `uncertain_write`, the published leaf and retained reconciliation
stage.

On the corrected source, CLI unit tests pass 21/21, including the four timing
and overwrite cases in the new deterministic test; the real-process
authorization tests pass 2/2. Strict workspace Clippy with warnings denied,
Cargo formatting, documentation and architecture checks, no-write inventory
and diff whitespace pass. The Python suite passes 42/43 tests with only the
same CM-R01.6 eight-input source-bound freshness failure; no digest or
package was renewed. Initial strict Clippy runs found an owned pass-by-value
lint and then test fixture naming/length lints. All three were corrected;
their failed outputs remain in private session evidence. No corrected-source
behavioral test failed.

The inventory was source/schema-reviewed and regenerated: 1,774 to 1,775
items, 825 explicit gaps before and after, with the normalized gap set
unchanged. The new public Rust surface is the guarded writer; three other
line-derived IDs moved and 18 existing entries changed only in location or
heuristic test mappings. These mappings are not coverage proof. This local
correction does not close the independent finding until the exact corrected
commit is reviewed.
