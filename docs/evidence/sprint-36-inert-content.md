# Sprint 36.3 inert-content implementation evidence

This is local evidence for Sub-task 36.3.2.3 under Decision 0022. It records the content and
interaction boundaries in the tested source tree. It does not qualify a real desktop browser,
screen reader or human workflow.

## Surface audit

The UI uses no Markdown or HTML renderer, image widget fed by repository/backend data, direct
hyperlink widget, web view or automatic URL opener. Ordinary egui labels show content as text.
The shared `content::render` path also bounds and sanitizes fields that can carry long or
model-influenced text:

| Surface | Data shown as inert text |
| --- | --- |
| Work plan | Bounded, control-safe sprint, story and item list titles; selected task detail |
| Campaign | Active task and configured model names; current model |
| Changes and evidence | Git subject and file name; review/gate summary; journal phases and activity lines |
| Campaign controls | Bounded coordinator activity lines |
| Reports | Bounded JSON preview; guarded export retains the full report |

The presenter replaces terminal and Unicode direction controls with a visible replacement
character, displays at most 4,096 characters in one source field, and labels any shortening.
List labels use the same bound without making link controls. The presenter offers
at most eight complete HTTPS link candidates per field. A candidate is shown in full next to
**Review external link**; the trusted shell then shows the full destination and requires
**Open in browser**. Neither rendering nor selecting a task, review or report opens the link.
Test fixtures cancel the dialog and make no browser request.

## Adversarial matrix

| Input | Expected local result |
| --- | --- |
| Script tags and a forged campaign button | Literal text only; no campaign control created |
| Markdown remote-image syntax | Literal text; no image or remote fetch |
| Terminal escape and Unicode direction override | Visible replacement characters |
| HTTP, user-info, numeric/local host, malformed or Unicode URL | No clickable link candidate |
| Complete external HTTPS DNS URL | Candidate offered; separate confirmation required |
| Content longer than 4,096 characters | Bounded preview; no link discovered beyond the bound |

The focused pure tests are in `crates/codingmage-ui/src/content.rs`; the offscreen trusted-chrome
interaction test is in `crates/codingmage-ui/tests/content.rs`. The strict-plan accessibility
regression is in `crates/codingmage-ui/tests/workplan.rs`: it supplies oversized sprint, story and
item titles with direction and terminal controls. These use synthetic hostile text,
not a live provider. The same screens still depend on the open command-boundary and complete
state/depth work in Sub-task 36.3.2.2.

## Verification disposition

The exact implementation revision is the commit containing this file. The following checks were
run against the candidate tree before that commit:

| Check | Result |
| --- | --- |
| `cargo test --locked -p codingmage-ui --lib content -- --test-threads=1` | 3 passed |
| `cargo test --locked -p codingmage-ui --test content -- --test-threads=1` | 1 passed; review click and Cancel exercised in offscreen egui |
| `cargo test --locked -p codingmage-ui --all-targets -- --test-threads=1` | 75 passed, 0 failed |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | Passed |
| `cargo fmt --all -- --check`, `python3 scripts/docs_check.py`, `python3 scripts/verification_inventory.py`, `git diff --check` | Passed; inventory refreshed to 1,675 surfaces / 811 explicit gaps |
| Python unittest suite | 41 passed, 1 retained CM-R01.6 source-bound evidence input-drift failure (eight paths outside this UI batch) |

For a clean target directory, build the sibling CLI **before** the all-target UI suite. The UI
fixture resolves the actual `codingmage` executable from that target directory. Run these in
order with the same `CARGO_TARGET_DIR` and any local resource wrapper required by the build host:

```sh
export CARGO_TARGET_DIR="$(mktemp -d)"
export CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=1
cargo build --locked -p codingmage-cli
cargo test --locked -p codingmage-ui --all-targets -- --test-threads=1
```

Every heavy Rust invocation used the shared build slot with one Cargo job and one Rust test
thread. The offscreen interaction test is synthetic and does not open a browser. The full UI suite
continues to use disposable repositories and fake providers. The previously retained workspace
process-reaping fixture failures and CM-R01.6 Python evidence drift remain separate open items;
this batch does not renew their evidence.
Independent review of `a0e90436e5189149a319de16010eb3d9b1252f82` was inconclusive and
identified three corrections: bounded list titles, shared confirmation geometry and a clean-target
CLI prerequisite. The source tree containing this note addresses those findings; the later
[command-preview evidence](sprint-36-command-previews.md) records the corrective 79-test UI run,
strict Clippy and retained Python failure. Independent re-review remains open. No Sprint 36
acceptance criterion or gate is closed by these local tests.
