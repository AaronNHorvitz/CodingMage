//! Bundled message catalogue and synthetic layout-stress variants.

use std::{collections::BTreeMap, sync::OnceLock};

use serde::Deserialize;

const SOURCE: &str = include_str!("../assets/messages/en.toml");
const REQUIRED_KEYS: &[&str] = &[
    "help_title",
    "help_offline",
    "help_return",
    "help_setup",
    "help_get_started",
    "help_open_configuration",
    "help_select_campaign",
    "help_attention",
    "help_missing_coordinator",
    "help_provider",
    "help_stale",
    "help_blocked",
    "help_keyboard_title",
    "help_keyboard_navigation",
    "help_keyboard_commands",
    "help_diagnostics_title",
    "help_diagnostics_description",
    "help_diagnostics_copy",
    "help_glossary_title",
    "help_term_campaign",
    "help_meaning_campaign",
    "help_term_admission",
    "help_meaning_admission",
    "help_term_preflight",
    "help_meaning_preflight",
    "help_term_source_checkbox",
    "help_meaning_source_checkbox",
    "help_term_gate",
    "help_meaning_gate",
    "help_term_stale",
    "help_meaning_stale",
    "help_term_blocker",
    "help_meaning_blocker",
    "help_term_support_bundle",
    "help_meaning_support_bundle",
    "help_about_title",
    "help_about_source_notice",
    "help_about_source_license",
    "help_about_third_party",
    "help_manual_diagnostics_title",
    "help_manual_diagnostics_description",
    "help_bundle_directory_label",
    "help_bundle_directory_hint",
    "help_bundle_create",
    "help_bundle_command_label",
    "help_bundle_pending",
    "help_bundle_unconfirmed",
    "help_bundle_recovery",
    "settings_title",
    "settings_intro",
    "settings_system",
    "settings_light",
    "settings_dark",
    "settings_high_contrast",
    "settings_palette_prefix",
    "settings_session_only",
    "overview_campaign_title",
    "overview_campaign_none",
    "overview_campaign_open",
    "overview_campaign_selected",
    "overview_campaign_observation",
    "overview_campaign_state",
    "overview_campaign_outcomes",
    "overview_campaign_completed",
    "overview_campaign_blocked",
    "overview_campaign_deferred",
    "overview_campaign_decisions",
    "overview_campaign_attention",
    "overview_campaign_attention_counts",
    "overview_campaign_no_attention",
    "overview_campaign_stale",
    "overview_campaign_not_started",
    "overview_campaign_loading",
    "overview_campaign_failed",
    "overview_campaign_unobserved",
    "blockers_title",
    "blockers_intro",
    "blockers_no_project",
    "blockers_no_campaign",
    "blockers_open_setup",
    "blockers_open_campaign",
    "blockers_open_work_plan",
    "blockers_open_help",
    "blockers_refresh",
    "blockers_refresh_unavailable",
    "blockers_observation",
    "blockers_loading",
    "blockers_stale",
    "blockers_failed",
    "blockers_unobserved",
    "blockers_not_started",
    "blockers_search",
    "blockers_search_hint",
    "blockers_campaign_hold",
    "blockers_campaign_action",
    "blockers_blocked_tasks",
    "blockers_task_action",
    "blockers_human_decisions",
    "blockers_human_action",
    "blockers_no_holds",
    "blockers_no_matches",
    "blockers_deferred_tasks",
    "blockers_trigger",
    "blockers_deferral_action",
    "blockers_narrow_search",
    "blockers_manual_boundary",
    "navigation_shortcuts",
    "blockers_state",
    "blockers_deferral_satisfied",
    "blockers_deferral_unknown",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema_version: u16,
    messages: BTreeMap<String, String>,
}

pub(crate) struct Catalogue {
    messages: BTreeMap<String, String>,
}

impl Catalogue {
    fn parse(source: &str) -> Result<Self, String> {
        let document: Document = toml::from_str(source).map_err(|error| error.to_string())?;
        if document.schema_version != 1 {
            return Err("unsupported message catalogue schema".to_owned());
        }
        if document.messages.len() != REQUIRED_KEYS.len()
            || REQUIRED_KEYS.iter().any(|key| {
                document
                    .messages
                    .get(*key)
                    .is_none_or(|value| value.trim().is_empty())
            })
        {
            return Err("message catalogue has a missing, unknown or empty key".to_owned());
        }
        Ok(Self {
            messages: document.messages,
        })
    }

    pub(crate) fn text(&self, key: &str) -> &str {
        self.messages
            .get(key)
            .unwrap_or_else(|| panic!("missing bundled UI message: {key}"))
    }

    #[cfg(test)]
    pub(crate) fn pseudo(&self, right_to_left: bool) -> Self {
        let messages = self
            .messages
            .iter()
            .map(|(key, value)| (key.clone(), expand(value, right_to_left)))
            .collect();
        Self { messages }
    }
}

#[cfg(test)]
fn expand(source: &str, right_to_left: bool) -> String {
    let mut result = if right_to_left {
        format!("אבג ⟦{source}⟧")
    } else {
        format!("⟦{source}⟧")
    };
    let minimum = source.chars().count().saturating_mul(7).div_ceil(5);
    while result.chars().count() < minimum {
        result.push('~');
    }
    result
}

pub(crate) fn english() -> &'static Catalogue {
    static ENGLISH: OnceLock<Catalogue> = OnceLock::new();
    ENGLISH.get_or_init(|| {
        Catalogue::parse(SOURCE).expect("the bundled English message catalogue is valid")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_messages_are_complete_and_pseudo_variants_stress_length_and_direction() {
        let english = english();
        let expanded = english.pseudo(false);
        let rtl = english.pseudo(true);
        for key in REQUIRED_KEYS {
            let base = english.text(key);
            let minimum = base.chars().count().saturating_mul(7).div_ceil(5);
            assert!(expanded.text(key).chars().count() >= minimum);
            assert!(rtl.text(key).chars().count() >= minimum);
            assert!(rtl.text(key).starts_with("אבג"));
        }
    }

    #[test]
    fn catalogue_rejects_missing_unknown_empty_and_unsupported_messages() {
        assert!(Catalogue::parse(&SOURCE.replace("help_title =", "help_other =")).is_err());
        assert!(
            Catalogue::parse(
                &SOURCE.replace("help_title = \"Help and About\"", "help_title = \"\"")
            )
            .is_err()
        );
        assert!(
            Catalogue::parse(&SOURCE.replace("schema_version = 1", "schema_version = 2")).is_err()
        );
        assert!(Catalogue::parse(&format!("{SOURCE}\nhelp_extra = \"unexpected\"\n")).is_err());
        assert!(Catalogue::parse(&format!("{SOURCE}\nhelp_title = \"duplicate\"\n")).is_err());
    }
}
