//! Searchable, read-only presentation of coordinator-owned campaign holds.

use super::{App, Screen, failure_box};
use crate::{
    backend::{
        explain_code,
        models::{BlockerExplanation, Deferral, TaskReason},
    },
    command, content,
    messages::{self, Catalogue},
    observed::{Freshness, age_label},
};

const VISIBLE_LIMIT: usize = 100;

impl App {
    pub(super) fn blockers_screen(&mut self, ui: &mut egui::Ui) {
        let messages = messages::english();
        ui.heading(messages.text("blockers_title"));
        ui.label(messages.text("blockers_intro"));
        if self.project.is_none() {
            ui.label(messages.text("blockers_no_project"));
            if ui.button(messages.text("blockers_open_setup")).clicked() {
                self.screen = Screen::Setup;
            }
            return;
        }
        if self.campaign.is_none() {
            ui.label(messages.text("blockers_no_campaign"));
            if ui.button(messages.text("blockers_open_campaign")).clicked() {
                self.screen = Screen::Campaign;
            }
            return;
        }
        self.blocker_refresh_controls(ui, messages);
        self.blocker_observation(ui, messages);
        ui.separator();
        ui.label(messages.text("blockers_manual_boundary"));
        ui.horizontal(|ui| {
            if ui
                .button(messages.text("blockers_open_work_plan"))
                .clicked()
            {
                self.screen = Screen::WorkPlan;
            }
            if ui.button(messages.text("blockers_open_campaign")).clicked() {
                self.screen = Screen::Campaign;
            }
            if ui.button(messages.text("blockers_open_help")).clicked() {
                self.screen = Screen::Help;
            }
        });
    }

    fn blocker_refresh_controls(&mut self, ui: &mut egui::Ui, messages: &Catalogue) {
        let arguments = self.explanation_arguments();
        let previewable = command::can_preview(self.binary_path.as_deref(), arguments.as_deref());
        if ui
            .add_enabled(
                previewable && !self.explanation.loading,
                egui::Button::new(messages.text("blockers_refresh")),
            )
            .clicked()
        {
            self.request_explanation();
        }
        if let Some(arguments) = arguments {
            command::show_for(
                ui,
                messages.text("blockers_refresh"),
                self.binary_path.as_deref(),
                &arguments,
            );
        } else {
            command::show_unavailable_for(ui, messages.text("blockers_refresh"));
        }
        if !previewable {
            ui.label(messages.text("blockers_refresh_unavailable"));
        }
    }

    fn blocker_observation(&mut self, ui: &mut egui::Ui, messages: &Catalogue) {
        let freshness = self.explanation.freshness(self.now);
        ui.small(format!(
            "{} {} ({})",
            messages.text("blockers_observation"),
            freshness.label(),
            age_label(self.explanation.age(self.now))
        ));
        if self.explanation.loading {
            ui.label(messages.text("blockers_loading"));
        }
        if freshness == Freshness::Stale {
            ui.label(messages.text("blockers_stale"));
        }
        if let Some((_, error)) = &self.explanation.last_error {
            let code = error.code();
            let (cause, action) = explain_code(&code);
            failure_box(
                ui,
                messages.text("blockers_failed"),
                &format!("{} ({})", cause, content::list_label(&code)),
                action,
            );
        }
        if self.explanation.value.as_ref().is_some_and(Option::is_some) {
            self.blocker_query_controls(ui, messages);
        }
        match &self.explanation.value {
            None if freshness == Freshness::NotRequested => {
                ui.label(messages.text("blockers_unobserved"));
            }
            None => {}
            Some(None) => {
                ui.label(messages.text("blockers_not_started"));
            }
            Some(Some(explanation)) => {
                render_explanation(ui, explanation, &self.blocker_query, messages);
            }
        }
    }

    fn blocker_query_controls(&mut self, ui: &mut egui::Ui, messages: &Catalogue) {
        let label = ui.label(messages.text("blockers_search"));
        ui.add(
            egui::TextEdit::singleline(&mut self.blocker_query)
                .hint_text(messages.text("blockers_search_hint"))
                .desired_width(super::current_tokens(ui.ctx()).layout.field_long),
        )
        .labelled_by(label.id);
        self.blocker_query = self.blocker_query.chars().take(120).collect();
    }
}

fn render_explanation(
    ui: &mut egui::Ui,
    explanation: &BlockerExplanation,
    query: &str,
    messages: &Catalogue,
) {
    let query = query.trim().to_lowercase();
    let mut matches = 0;
    ui.label(format!(
        "{} {}",
        messages.text("blockers_state"),
        content::list_label(&explanation.state)
    ));
    if let Some(code) = &explanation.blocker_code
        && field_matches(code, &query)
    {
        ui.strong(messages.text("blockers_campaign_hold"));
        ui.monospace(content::list_label(code));
        ui.label(messages.text("blockers_campaign_action"));
        matches += 1;
    }
    matches += render_reasons(
        ui,
        messages.text("blockers_blocked_tasks"),
        &explanation.blockers,
        &query,
        blocker_action,
        messages.text("blockers_narrow_search"),
        messages,
    );
    matches += render_deferrals(ui, &explanation.deferrals, &query, messages);
    matches += render_reasons(
        ui,
        messages.text("blockers_human_decisions"),
        &explanation.human_decisions,
        &query,
        human_action,
        messages.text("blockers_narrow_search"),
        messages,
    );
    if matches == 0 {
        let no_holds = explanation.blocker_code.is_none()
            && explanation.blockers.is_empty()
            && explanation.deferrals.is_empty()
            && explanation.human_decisions.is_empty();
        ui.label(messages.text(if no_holds {
            "blockers_no_holds"
        } else {
            "blockers_no_matches"
        }));
    }
}

fn render_reasons(
    ui: &mut egui::Ui,
    title: &str,
    reasons: &[TaskReason],
    query: &str,
    action: fn(&str) -> &'static str,
    narrow: &str,
    messages: &Catalogue,
) -> usize {
    let matched = reasons
        .iter()
        .filter(|reason| {
            field_matches(&reason.task_id, query) || field_matches(&reason.reason_code, query)
        })
        .collect::<Vec<_>>();
    if !matched.is_empty() {
        ui.strong(format!("{} ({})", title, matched.len()));
        for reason in matched.iter().take(VISIBLE_LIMIT) {
            ui.horizontal_wrapped(|ui| {
                ui.monospace(content::list_label(&reason.task_id));
                ui.label(content::list_label(&reason.reason_code));
            });
            ui.small(messages.text(action(&reason.reason_code)));
        }
        if matched.len() > VISIBLE_LIMIT {
            ui.small(narrow);
        }
    }
    matched.len()
}

fn render_deferrals(
    ui: &mut egui::Ui,
    deferrals: &[Deferral],
    query: &str,
    messages: &Catalogue,
) -> usize {
    let matched = deferrals
        .iter()
        .filter(|item| {
            field_matches(&item.task_id, query)
                || field_matches(&item.reason_code, query)
                || field_matches(&item.trigger_code, query)
                || field_matches(&item.trigger_state, query)
        })
        .collect::<Vec<_>>();
    if !matched.is_empty() {
        ui.strong(format!(
            "{} ({})",
            messages.text("blockers_deferred_tasks"),
            matched.len()
        ));
        for item in matched.iter().take(VISIBLE_LIMIT) {
            ui.horizontal_wrapped(|ui| {
                ui.monospace(content::list_label(&item.task_id));
                ui.label(content::list_label(&item.reason_code));
                ui.label(format!(
                    "{}: {} ({})",
                    messages.text("blockers_trigger"),
                    content::list_label(&item.trigger_code),
                    content::list_label(&item.trigger_state)
                ));
            });
            let action =
                deferral_action(&item.reason_code, &item.trigger_code, &item.trigger_state);
            ui.small(messages.text(action));
        }
        if matched.len() > VISIBLE_LIMIT {
            ui.small(messages.text("blockers_narrow_search"));
        }
    }
    matched.len()
}

fn field_matches(field: &str, query: &str) -> bool {
    query.is_empty() || field.to_lowercase().contains(query)
}

// These codes are a presentation copy of the coordinator's closed reason contract. Unknown
// codes get no clearance advice: the coordinator, not this view, owns their interpretation.
fn blocker_action(code: &str) -> &'static str {
    match code {
        "unavailable_external_dependency" => "blockers_action_external_dependency",
        "unavailable_supported_hardware" => "blockers_action_hardware",
        "missing_operator_managed_authentication" => "blockers_action_authentication",
        "unavailable_external_service" => "blockers_action_service",
        "unsupported_platform" => "blockers_action_platform",
        "blocked_prerequisite" => "blockers_action_prerequisite",
        "implementation_condition_outside_authority" => "blockers_action_authority",
        _ => "blockers_task_action",
    }
}

fn human_action(code: &str) -> &'static str {
    match code {
        "ambiguous_scope" => "blockers_decision_scope",
        "material_architecture_choice" => "blockers_decision_architecture",
        "requested_authority_expansion" => "blockers_decision_authority",
        "protected_branch_consequence" => "blockers_decision_branch",
        "external_infrastructure_change" => "blockers_decision_infrastructure",
        "release_decision" => "blockers_decision_release",
        "repeated_satisfied_deferral" => "blockers_decision_repeated",
        _ => "blockers_human_action",
    }
}

fn deferral_action(reason: &str, trigger: &str, state: &str) -> &'static str {
    let expected = match reason {
        "temporary_provider_capacity" => "provider_reset",
        "active_path_lease" => "lease_release",
        "gate_resource_contention" => "gate_resource_release",
        "deterministic_dependency_order" => "campaign_head_advancement",
        "pending_stronger_review" => "review_completion",
        "operator_pause" => "operator_resume",
        _ => return "blockers_deferral_unknown",
    };
    if trigger != expected {
        return "blockers_deferral_unknown";
    }
    match state {
        "satisfied" => "blockers_deferral_satisfied",
        "pending" => match trigger {
            // Only these three triggers have an explicit evidence-bound observation command.
            "provider_reset" => "blockers_trigger_provider_reset",
            "review_completion" => "blockers_trigger_review_completion",
            "operator_resume" => "blockers_trigger_operator_resume",
            // The coordinator observes these while it advances or releases its own resources.
            "campaign_head_advancement" => "blockers_trigger_head",
            "lease_release" => "blockers_trigger_lease",
            "gate_resource_release" => "blockers_trigger_gate",
            _ => "blockers_deferral_unknown",
        },
        _ => "blockers_deferral_unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::{blocker_action, deferral_action, human_action};
    use codingmage_contracts::{LeadBlockedReason, LeadDeferredReason, LeadHumanDecisionReason};

    #[test]
    fn only_externally_observable_pending_triggers_offer_manual_evidence() {
        for (reason, trigger, action) in [
            (
                "temporary_provider_capacity",
                "provider_reset",
                "blockers_trigger_provider_reset",
            ),
            (
                "pending_stronger_review",
                "review_completion",
                "blockers_trigger_review_completion",
            ),
            (
                "operator_pause",
                "operator_resume",
                "blockers_trigger_operator_resume",
            ),
            (
                "deterministic_dependency_order",
                "campaign_head_advancement",
                "blockers_trigger_head",
            ),
            (
                "active_path_lease",
                "lease_release",
                "blockers_trigger_lease",
            ),
            (
                "gate_resource_contention",
                "gate_resource_release",
                "blockers_trigger_gate",
            ),
        ] {
            assert_eq!(deferral_action(reason, trigger, "pending"), action);
            assert_eq!(
                deferral_action(reason, trigger, "satisfied"),
                "blockers_deferral_satisfied"
            );
        }
        for (reason, trigger, state) in [
            ("temporary_provider_capacity", "new_trigger", "pending"),
            ("temporary_provider_capacity", "lease_release", "pending"),
            ("new_reason", "provider_reset", "pending"),
            ("temporary_provider_capacity", "provider_reset", "unknown"),
            ("new_reason", "new_trigger", "satisfied"),
        ] {
            assert_eq!(
                deferral_action(reason, trigger, state),
                "blockers_deferral_unknown"
            );
        }
    }

    #[test]
    fn unknown_and_prerequisite_codes_do_not_offer_external_clearance() {
        assert_eq!(blocker_action("new_reason"), "blockers_task_action");
        assert_eq!(
            blocker_action("blocked_prerequisite"),
            "blockers_action_prerequisite"
        );
        assert_eq!(human_action("new_decision"), "blockers_human_action");
    }

    #[test]
    fn every_closed_reason_has_specific_guidance() {
        for reason in [
            LeadBlockedReason::UnavailableExternalDependency,
            LeadBlockedReason::UnavailableSupportedHardware,
            LeadBlockedReason::MissingOperatorManagedAuthentication,
            LeadBlockedReason::UnavailableExternalService,
            LeadBlockedReason::UnsupportedPlatform,
            LeadBlockedReason::BlockedPrerequisite,
            LeadBlockedReason::ImplementationConditionOutsideAuthority,
        ] {
            assert_ne!(blocker_action(reason.code()), "blockers_task_action");
        }
        for reason in [
            LeadHumanDecisionReason::AmbiguousScope,
            LeadHumanDecisionReason::MaterialArchitectureChoice,
            LeadHumanDecisionReason::RequestedAuthorityExpansion,
            LeadHumanDecisionReason::ProtectedBranchConsequence,
            LeadHumanDecisionReason::ExternalInfrastructureChange,
            LeadHumanDecisionReason::ReleaseDecision,
        ] {
            assert_ne!(human_action(reason.code()), "blockers_human_action");
        }
        assert_ne!(
            human_action("repeated_satisfied_deferral"),
            "blockers_human_action"
        );
        for reason in [
            LeadDeferredReason::TemporaryProviderCapacity,
            LeadDeferredReason::ActivePathLease,
            LeadDeferredReason::GateResourceContention,
            LeadDeferredReason::DeterministicDependencyOrder,
            LeadDeferredReason::PendingStrongerReview,
            LeadDeferredReason::OperatorPause,
        ] {
            assert_ne!(
                deferral_action(reason.code(), reason.required_trigger().code(), "pending"),
                "blockers_deferral_unknown"
            );
        }
    }
}
