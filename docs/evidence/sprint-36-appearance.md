# Sprint 36.3 appearance and minimum-window evidence

This is local implementation evidence for Sub-task 36.3.2.1, based on the source after
`d371c73e46c65c7c342f64f0a2b17245647098f5`. The commit containing this file identifies
the exact tested code tree. The existing Sprint 36 screenshots remain historical evidence of the
older 720 by 480 shell and are not the new appearance baselines.

## Implemented surface

- `crates/codingmage-ui/src/design.rs` is the single Rust source of colour, type-size, spacing
  and radius tokens. It resolves system, light, dark and high-contrast appearances and applies
  the result to both egui theme styles. Optional animation time is zero.
- The native viewport minimum is 1024 by 640 logical pixels. Settings offers four accessible
  radio choices for the current window. A shared failure frame and screen status text use the
  semantic colours. Status meaning remains in words.
- The status bar says that the coordinator is ready without exposing its private executable path
  in a screenshot. The command path still exists internally for controlled launch requests.

## Measured contrast

The `design::tests::all_semantic_text_and_focus_tokens_meet_contrast_targets` test calculates
sRGB relative luminance and checks every semantic text colour on canvas, panel and control
surfaces. It requires at least 4.5:1 for text and 3:1 for the focus indicator. The worst computed
ratios are:

| Appearance | Lowest text ratio | Lowest focus ratio |
| --- | ---: | ---: |
| Light | 5.88:1 | 6.62:1 |
| Dark | 6.66:1 | 9.06:1 |
| High contrast | 11.51:1 | 16.85:1 |

The selected-control fill is a control surface with tested text contrast, and its focus outline
uses the focus token. These numbers cover declared tokens, not every pixel produced by egui or a
compositor. Disabled controls are not treated as active controls in this measurement.

## Inspected screenshot baselines

The `appearance_baselines_cover_all_palettes_at_minimum_size_and_double_scale` test renders the
Settings screen through `egui_kittest` and offscreen `wgpu` on Mesa llvmpipe (CPU, Vulkan). It
clicks each accessible radio choice, confirms the selected palette label, checks output
dimensions and requires six distinct pixel digests. The images were visually inspected for
legible text, visible selection/focus and absence of machine paths. The table gives SHA-256 of
the committed PNG bytes; the private test manifest also retains raw-pixel digests.

| Appearance | 100%, 1024×640 | 200%, 2048×1280 |
| --- | --- | --- |
| Light | [image](sprint-36-appearance/light-100.png) `387d8c0069627911` | [image](sprint-36-appearance/light-200.png) `89b3d94128b87ddc` |
| Dark | [image](sprint-36-appearance/dark-100.png) `69c98601a2c14432` | [image](sprint-36-appearance/dark-200.png) `d52c59fe3776247c` |
| High contrast | [image](sprint-36-appearance/contrast-100.png) `4dedeb52023f6c02` | [image](sprint-36-appearance/contrast-200.png) `72069bbb3d8577af` |

These are synthetic offscreen baselines. They do not prove compositor, Orca, real desktop input,
all-screen contrast or the frozen P1–P5 responsiveness budgets. Those remain open under Story
36.3 and the human-only register.

## Reproduction and disposition

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --locked -p codingmage-cli
CODINGMAGE_UI_EVIDENCE_DIR=<private directory> cargo test --locked -p codingmage-ui --lib --test verification -- --test-threads=1
python3 scripts/docs_check.py
python3 -m unittest discover -s tests -p 'test_*.py'
git diff --check
```

All heavy Rust commands run with `CARGO_BUILD_JOBS=1`, `RUST_TEST_THREADS=1` and the shared
build-slot wrapper in this worker. The CLI build is required for a clean test target because
the UI integration fixtures launch the sibling `codingmage` executable. The Python suite's
retained CM-R01.6 source-bound drift failure is separate from this UI change. The results for
this source tree are:

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| Strict workspace Clippy, all targets | Pass |
| `cargo test --locked --workspace --all-targets -- --test-threads=1` | 235 passed, 2 failed, 2 ignored before Cargo stopped at the known sandbox process-reaping fixture timeouts; no earlier failures |
| `cargo test --locked -p codingmage-ui --all-targets -- --test-threads=1` | 71 passed, 0 failed |
| `python3 scripts/docs_check.py` | Pass |
| `python3 scripts/verification_inventory.py` | Pass, 1,668 surfaces and 811 explicit gaps |
| Python unittest suite | 41 passed, 1 retained failure in `test_multi_agent_evidence_binding_is_current` (eight `input-drift` paths; open CM-R01.6) |
| `git diff --check` | Pass |

No human or independent review is claimed here.
