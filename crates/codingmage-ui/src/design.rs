//! Shared native visual tokens and appearance selection.

use egui::{Color32, Context, CornerRadius, FontId, Stroke, TextStyle, Vec2, Visuals};

/// User-selected appearance. System follows the desktop preference when exposed by egui.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Appearance {
    /// Follow the desktop preference.
    #[default]
    System,
    /// Use the light palette.
    Light,
    /// Use the dark palette.
    Dark,
    /// Use the high-contrast palette.
    HighContrast,
}

impl Appearance {
    /// Choices in presentation order.
    pub const ALL: [Self; 4] = [Self::System, Self::Light, Self::Dark, Self::HighContrast];

    /// Accessible label for the appearance control.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::System => "Follow system",
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::HighContrast => "High contrast",
        }
    }

    /// Resolves the effective palette without changing the preference.
    #[must_use]
    pub fn resolve(self, ctx: &Context, fallback_dark: bool) -> Palette {
        match self {
            Self::System => {
                if ctx
                    .system_theme()
                    .is_some_and(|theme| theme == egui::Theme::Light)
                {
                    Palette::Light
                } else if ctx
                    .system_theme()
                    .is_some_and(|theme| theme == egui::Theme::Dark)
                    || fallback_dark
                {
                    Palette::Dark
                } else {
                    Palette::Light
                }
            }
            Self::Light => Palette::Light,
            Self::Dark => Palette::Dark,
            Self::HighContrast => Palette::HighContrast,
        }
    }
}

/// Resolved palette used for the current frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Palette {
    /// Light colours.
    Light,
    /// Dark colours.
    Dark,
    /// Maximum contrast colours.
    HighContrast,
}

impl Palette {
    /// User-facing palette name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::HighContrast => "High contrast",
        }
    }
}

/// Single token source for native colors and geometry.
#[derive(Clone, Copy, Debug)]
pub struct Tokens {
    /// Main canvas.
    pub canvas: Color32,
    /// Panel background.
    pub panel: Color32,
    /// Interactive surface.
    pub control: Color32,
    /// Primary text.
    pub text: Color32,
    /// Secondary text.
    pub muted: Color32,
    /// Highlight and selected control.
    pub accent: Color32,
    /// Visible focus indicator.
    pub focus: Color32,
    /// Error text.
    pub error: Color32,
    /// Warning text.
    pub warning: Color32,
    /// Success text.
    pub success: Color32,
    /// Item spacing in logical pixels.
    pub item_spacing: Vec2,
    /// Button padding in logical pixels.
    pub button_padding: Vec2,
    /// Named dimensions shared by all application screens.
    pub layout: LayoutTokens,
    /// Control corner radius in logical pixels.
    pub radius: u8,
    /// Heading font size in logical pixels.
    pub heading_size: f32,
    /// Body and button font size in logical pixels.
    pub body_size: f32,
    /// Small annotation font size in logical pixels.
    pub small_size: f32,
    /// Monospace font size in logical pixels.
    pub mono_size: f32,
}

/// Screen layout dimensions in logical pixels.
#[derive(Clone, Copy, Debug)]
pub struct LayoutTokens {
    /// Spacing for ordinary label/value grids.
    pub grid: Vec2,
    /// Spacing for compact label/value grids.
    pub grid_compact: Vec2,
    /// Spacing for dense lists of files.
    pub grid_dense: Vec2,
    /// Space between navigation groups.
    pub navigation_gap: f32,
    /// Width of the navigation rail.
    pub navigation_width: f32,
    /// Space between major sections.
    pub section_gap: f32,
    /// Maximum height of short previews.
    pub preview_short: f32,
    /// Maximum height of record previews.
    pub preview_records: f32,
    /// Maximum height of task previews.
    pub preview_tasks: f32,
    /// Maximum height of report previews.
    pub preview_reports: f32,
    /// Maximum height of expanded previews.
    pub preview_tall: f32,
    /// Width of the narrowest form field.
    pub field_tiny: f32,
    /// Width of a short form field.
    pub field_short: f32,
    /// Width of a small form field.
    pub field_small: f32,
    /// Width of a medium form field.
    pub field_medium: f32,
    /// Width of a label field.
    pub field_label: f32,
    /// Width of a standard form field.
    pub field_standard: f32,
    /// Width of a path field.
    pub field_path: f32,
    /// Width of a long form field.
    pub field_long: f32,
    /// Width of a full form field.
    pub field_full: f32,
    /// Width of a wide form field.
    pub field_wide: f32,
}

impl Default for LayoutTokens {
    fn default() -> Self {
        Self {
            grid: Vec2::new(12.0, 6.0),
            grid_compact: Vec2::new(12.0, 4.0),
            grid_dense: Vec2::new(12.0, 2.0),
            navigation_gap: 4.0,
            navigation_width: 170.0,
            section_gap: 12.0,
            preview_short: 180.0,
            preview_records: 200.0,
            preview_tasks: 240.0,
            preview_reports: 300.0,
            preview_tall: 360.0,
            field_tiny: 100.0,
            field_short: 160.0,
            field_small: 200.0,
            field_medium: 240.0,
            field_label: 260.0,
            field_standard: 300.0,
            field_path: 360.0,
            field_long: 420.0,
            field_full: 480.0,
            field_wide: 600.0,
        }
    }
}

impl Tokens {
    /// Returns the fixed colors and geometry for a palette.
    #[must_use]
    pub fn for_palette(palette: Palette) -> Self {
        let rgb = Color32::from_rgb;
        let base = match palette {
            Palette::Light => (
                rgb(247, 249, 252),
                Color32::WHITE,
                rgb(230, 235, 242),
                rgb(24, 34, 49),
                rgb(65, 77, 92),
                rgb(6, 77, 147),
                rgb(120, 69, 0),
                rgb(159, 30, 38),
                rgb(117, 68, 0),
                rgb(20, 102, 53),
            ),
            Palette::Dark => (
                rgb(16, 21, 31),
                rgb(25, 34, 49),
                rgb(41, 54, 75),
                rgb(245, 247, 250),
                rgb(191, 201, 214),
                rgb(124, 199, 255),
                rgb(255, 219, 112),
                rgb(255, 171, 178),
                rgb(255, 211, 133),
                rgb(150, 232, 181),
            ),
            Palette::HighContrast => (
                Color32::BLACK,
                Color32::BLACK,
                rgb(22, 22, 22),
                Color32::WHITE,
                rgb(230, 230, 230),
                rgb(0, 255, 255),
                Color32::YELLOW,
                rgb(255, 190, 190),
                rgb(255, 225, 150),
                rgb(180, 255, 180),
            ),
        };
        Self {
            canvas: base.0,
            panel: base.1,
            control: base.2,
            text: base.3,
            muted: base.4,
            accent: base.5,
            focus: base.6,
            error: base.7,
            warning: base.8,
            success: base.9,
            item_spacing: Vec2::new(8.0, 8.0),
            button_padding: Vec2::new(10.0, 6.0),
            layout: LayoutTokens::default(),
            radius: 6,
            heading_size: 20.0,
            body_size: 14.0,
            small_size: 11.0,
            mono_size: 13.0,
        }
    }

    /// Applies these tokens to egui's shared style.
    pub fn apply(self, ctx: &Context, palette: Palette) {
        let mut visuals = match palette {
            Palette::Light => Visuals::light(),
            Palette::Dark | Palette::HighContrast => Visuals::dark(),
        };
        visuals.panel_fill = self.panel;
        visuals.window_fill = self.panel;
        visuals.extreme_bg_color = self.canvas;
        visuals.override_text_color = Some(self.text);
        visuals.hyperlink_color = self.accent;
        visuals.error_fg_color = self.error;
        visuals.warn_fg_color = self.warning;
        visuals.selection.bg_fill = self.control;
        visuals.selection.stroke = Stroke::new(2.0, self.focus);
        visuals.widgets.noninteractive.bg_fill = self.panel;
        visuals.widgets.inactive.bg_fill = self.control;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, self.muted);
        visuals.widgets.hovered.bg_fill = self.control;
        visuals.widgets.active.bg_fill = self.control;
        for widget in [
            &mut visuals.widgets.noninteractive,
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
        ] {
            widget.fg_stroke.color = self.text;
            widget.corner_radius = CornerRadius::same(self.radius);
        }
        visuals.widgets.hovered.bg_stroke = Stroke::new(2.0, self.focus);
        visuals.widgets.active.bg_stroke = Stroke::new(2.0, self.focus);
        for theme in [egui::Theme::Light, egui::Theme::Dark] {
            let mut style = (*ctx.style_of(theme)).clone();
            style.visuals = visuals.clone();
            style.spacing.item_spacing = self.item_spacing;
            style.spacing.button_padding = self.button_padding;
            style.text_styles = std::collections::BTreeMap::from([
                (TextStyle::Heading, FontId::proportional(self.heading_size)),
                (TextStyle::Body, FontId::proportional(self.body_size)),
                (TextStyle::Button, FontId::proportional(self.body_size)),
                (TextStyle::Small, FontId::proportional(self.small_size)),
                (TextStyle::Monospace, FontId::monospace(self.mono_size)),
            ]);
            // The shell has no essential animation; keep focus and content movement immediate.
            style.animation_time = 0.0;
            ctx.set_style_of(theme, style);
        }
        ctx.data_mut(|data| data.insert_temp(egui::Id::new("codingmage.visual-palette"), palette));
    }
}

/// Returns the current shared tokens from a UI context.
#[must_use]
pub fn current_tokens(ctx: &Context) -> Tokens {
    let palette =
        ctx.data(|data| data.get_temp::<Palette>(egui::Id::new("codingmage.visual-palette")));
    Tokens::for_palette(palette.unwrap_or(Palette::Dark))
}

/// Relative WCAG contrast of two opaque sRGB token colours.
#[must_use]
pub fn contrast_ratio(a: Color32, b: Color32) -> f64 {
    fn luminance(color: Color32) -> f64 {
        let linear = |component: u8| {
            let value = f64::from(component) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(color.r()) + 0.7152 * linear(color.g()) + 0.0722 * linear(color.b())
    }
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_semantic_text_and_focus_tokens_meet_contrast_targets() {
        for palette in [Palette::Light, Palette::Dark, Palette::HighContrast] {
            let t = Tokens::for_palette(palette);
            for background in [t.canvas, t.panel, t.control] {
                for foreground in [t.text, t.muted, t.error, t.warning, t.success] {
                    assert!(
                        contrast_ratio(foreground, background) >= 4.5,
                        "{palette:?}: {foreground:?} on {background:?}"
                    );
                }
                assert!(contrast_ratio(t.focus, background) >= 3.0);
                assert!(contrast_ratio(t.accent, background) >= 4.5);
            }
        }
    }
}
