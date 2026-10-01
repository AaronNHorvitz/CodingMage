# Public Setup command boundary evidence

## First public write: authorization record

Task 36.3.2.2 remains open. Decision 0064 introduces the public
`setup-write-authorization` command as the first bounded Setup write. Its
arguments identify an existing configuration, the exact repository ID observed
by `doctor`, and a requested output outside that repository. A stale or
cross-project identity is refused before stdin is read. The held repository
identity and Git HEAD are revalidated after reading stdin. The exact record is
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
