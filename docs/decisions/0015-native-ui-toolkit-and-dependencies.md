# Decision 0015: Native Linux UI Toolkit and Dependency Admission

- **Status:** Accepted for the Sprint 36 native UI workstream
- **Date:** 2026-09-21
- **Decision owners:** Repository owner (implementation choice delegated to the assigned UI agent)
- **Supersedes:** None
- **Superseded by:** None

## Context

The owner selected a native Linux desktop app over the existing coordinator (PRD section
"Native Linux Desktop Interface", Sprint 36). The brief requires an established Rust-native
toolkit with a permissive licence, AccessKit accessibility, working Wayland and X11 backends,
reactive repaint, no in-house toolkit, no embedded browser, and dependency licences limited to
MIT, Apache-2.0, BSD, ISC, Zlib, Unicode or MPL-2.0. The workspace forbids `unsafe_code`, denies
`missing_docs`, warnings and strict Clippy, and pins exact dependency versions.

The development sandbox has no display server, no GPU and no root access, but it has Mesa
software rasterizers (lavapipe Vulkan, llvmpipe GL), system TrueType fonts and access to the
public crates.io index.

## Decision

1. Use `egui` 0.36.2 with `eframe` 0.36.2 as the presentation toolkit (MIT OR Apache-2.0).
   eframe provides the winit window shell with Wayland and X11 backends, AccessKit integration
   through `egui-winit`/`accesskit_winit`, and reactive repaint (an idle window issues no frames
   until input, a backend result or a scheduled repaint arrives).
2. Use the `wgpu` renderer (Vulkan first, GL fallback). The same renderer produces the offscreen
   test snapshots in the sandbox through `egui_kittest`, so screenshots come from the production
   rendering path on a software adapter and are labelled as such.
3. Disable bundled fonts. `egui`'s `default_fonts` feature and winit's `wayland-csd-adwaita`
   title renderer embed fonts under OFL-1.1 and the Ubuntu Font Licence, which are outside the
   admitted licence list. The app loads a sans-serif and a monospace font from standard system
   font directories at startup (`fonts.rs`) and fails visibly with an actionable message when no
   font is found. Wayland client-side decorations use `wayland-csd-adwaita-notitle` (no title
   text in the frame; the compositor and task switcher still show the window title).
4. Every crate reachable from `codingmage-ui` on `x86_64-unknown-linux-gnu` was audited from the
   locked graph. All 210 new runtime crates and 23 dev/build-only crates carry licence
   expressions that permit at least one admitted licence (MIT, Apache-2.0, BSD-2/3-Clause, ISC,
   Zlib, 0BSD or MPL-2.0 for the dev-only `colored` crate). No crate in the graph is licensed only
   under GPL, CC0, OFL or another unlisted licence. The audit inventory is retained as private
   working state and is reproducible with `cargo tree -p codingmage-ui --edges normal`.
5. Direct dependencies and their pinned versions:

   | Crate | Version | Licence | Scope | Purpose |
   | --- | --- | --- | --- | --- |
   | `egui` | 0.36.2 | MIT OR Apache-2.0 | runtime | Immediate-mode widgets and accessibility tree |
   | `eframe` | 0.36.2 | MIT OR Apache-2.0 | runtime | Native window shell, wgpu, AccessKit |
   | `winit` | 0.30.13 | Apache-2.0 | runtime (feature pin only) | Select `wayland-csd-adwaita-notitle`, `wayland-dlopen`, `rwh_06` |
   | `serde`, `serde_json`, `toml`, `sha2` | workspace pins | MIT OR Apache-2.0 | runtime | Existing workspace contracts |
   | `egui_kittest` | 0.36.2 | MIT OR Apache-2.0 | dev | Headless harness, AccessKit queries, wgpu snapshots |
   | `codingmage-runtime` | workspace | Apache-2.0 | dev | Contract-parity tests against the real backend types |

6. `codingmage-ui` may depend at runtime only on `codingmage-campaign`, `codingmage-contracts`,
   `codingmage-core` and `codingmage-plan` (parsers and validators without repository or process
   authority). The dependency policy records this edge set; it does not grant the UI a path to
   `codingmage-runtime`, `codingmage-git` or `codingmage-process` at runtime.

## Alternatives Considered

- **GTK4 (`gtk4-rs`, LGPL-2.1+)**: mature accessibility, but LGPL is outside the admitted licence
  list and the bindings need a native C toolkit and development headers on every build host.
- **Slint**: offered under GPL-3.0 or its own royalty-free and commercial licences; not one of the
  admitted plain permissive licences.
- **Iced**: MIT and winit-based, but it lacks an established headless harness comparable to
  `egui_kittest` for keyboard, accessibility-tree and snapshot tests in a display-less sandbox,
  and the brief names egui as the expected choice already used by the sibling product.
- **Dioxus/Tauri (webview)**: an embedded browser, excluded by the owner.
- **Bundled fonts**: rejected on licence grounds; system fonts are loaded at runtime instead.
- **`glow` renderer for production with `wgpu` only in tests**: rejected because screenshots would
  not come from the production renderer.

## Consequences

- A window requires a desktop session with Wayland or X11; the sandbox cannot show one. Native
  screenshots for evidence are rendered offscreen with the lavapipe software adapter and labelled
  as such. Real-desktop launch and screen-reader verification remain human-only items.
- The wgpu graph is large (about 210 crates). Build memory stays within the assigned limits at
  three jobs; release builds use the workspace profile.
- Font discovery is a runtime requirement documented in the installation notes. A `CODINGMAGE_UI_FONT`
  override points at one specific TrueType/OpenType file for unusual installations.
- Clipboard support (`arboard`) is enabled so digests and codes can be copied; it never reads the
  clipboard on its own.

## Verification

- `cargo tree -p codingmage-ui --edges normal` matches the audited inventory and contains no
  `epaint_default_fonts`, `crossfont` or font-embedding crate.
- The workspace lints (`unsafe_code = "forbid"`, `missing_docs = "deny"`, Clippy pedantic) apply
  to the crate.
- `egui_kittest` harness tests exercise keyboard navigation and the AccessKit tree without a
  display server; wgpu snapshot tests render on the software adapter.
