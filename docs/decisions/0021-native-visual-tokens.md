# ADR 0021: Native Visual Tokens and Appearance

- **Status:** Accepted for local implementation
- **Date:** 2026-09-27
- **Decision owners:** CodingMage implementation worker under Story 36.3
- **Supersedes:** None

## Context

The implemented native shell had fixed colours scattered through its screens, one implicit
appearance, and a 720 by 480 minimum window. ADR 0020 requires a single design system, light,
dark and high-contrast appearances, a 1024 by 640 minimum and measured contrast. A different
toolkit or a new style dependency would add an unnecessary boundary to the existing egui client.

## Decision

Keep colours, type sizes, spacing and corner radius in `codingmage-ui::design::Tokens`. Apply a
resolved palette to both egui theme styles so appearance is consistent if the desktop changes
theme. The default preference follows the desktop theme reported by egui; a missing signal uses
the initial dark fallback. Offer explicit light, dark and high-contrast choices in Settings for
the current window. Status meaning remains in text, with semantic colours as an additional cue.

Disable egui's optional animation time because the workspace requires no animated meaning and
immediate changes support reduced motion. Render untrusted content with egui's ordinary inert text
widgets. Keep the appearance selection in presentation state; persistent settings and localization
remain separate Story 36.3 work. Raise the native minimum to 1024 by 640 logical pixels.

## Verification and limits

Automated token checks require at least 4.5:1 for semantic text and accent against every declared
surface and 3:1 for focus. Offscreen software-rendered screenshots at 100% and 200% scale on the
minimum window are baseline artifacts, not Wayland/X11, Orca or human qualification. The full
screen/state and device matrix remains open under Story 36.3. No dependency was added.
