//! Fixed, inert command-palette entries for native navigation and read-only diagnosis.

use super::Screen;

#[derive(Default)]
pub(super) struct CommandPalette {
    pub(super) open: bool,
    pub(super) query: String,
    pub(super) index: usize,
    pub(super) focus_pending: bool,
    pub(super) search_id: Option<egui::Id>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PaletteAction {
    Navigate(Screen),
    RefreshDiagnosis,
}

impl PaletteAction {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Navigate(screen) => screen.label(),
            Self::RefreshDiagnosis => "Refresh diagnosis",
        }
    }
}

pub(super) fn matching_actions(query: &str) -> Vec<PaletteAction> {
    let query = query.trim().to_lowercase();
    Screen::ALL
        .into_iter()
        .map(PaletteAction::Navigate)
        .chain(std::iter::once(PaletteAction::RefreshDiagnosis))
        .filter(|action| action.label().to_lowercase().contains(&query))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_filters_fixed_destinations_and_action_without_repository_text() {
        assert_eq!(
            matching_actions("  bLoCk "),
            vec![PaletteAction::Navigate(Screen::Blockers)]
        );
        assert_eq!(
            matching_actions("diagnosis"),
            vec![PaletteAction::RefreshDiagnosis]
        );
        assert!(matching_actions("unrecognized repository instruction").is_empty());
        assert_eq!(matching_actions("").len(), Screen::ALL.len() + 1);
    }
}
