//! Bundled message catalogue and synthetic layout-stress variants.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};

use serde::Deserialize;

use crate::backend::FailureState;

const SOURCE: &str = include_str!("../assets/messages/en.toml");
const REQUIRED_KEYS: &[&str] = &[
    "failure_executable_missing",
    "failure_permission_denied",
    "failure_authentication_required",
    "failure_provider_unavailable",
    "failure_malformed_output",
    "failure_identity_mismatch",
    "failure_unsupported_schema",
    "failure_identity_mismatch_what",
    "failure_identity_mismatch_action",
    "failure_unsupported_schema_what",
    "failure_unsupported_schema_action",
    "failure_output_too_large",
    "failure_coordinator_unavailable",
    "failure_coordinator_timed_out",
    "failure_request_failed",
    "failure_diagnosis_no_observation",
    "failure_diagnosis_retained",
    "failure_preflight_no_observation",
    "failure_preflight_retained",
    "failure_changes_no_observation",
    "failure_records_no_observation",
    "readiness_title",
    "readiness_authorization",
    "readiness_authorization_hint",
    "readiness_use_record",
    "readiness_no_campaign",
    "readiness_local_checks",
    "readiness_run_preflight",
    "readiness_command_unavailable",
    "readiness_observation",
    "readiness_report_state",
    "readiness_copy_digest",
    "readiness_authority_digest",
    "readiness_authorization_digest",
    "readiness_repository",
    "readiness_repository_detail",
    "readiness_policy",
    "readiness_policy_detail",
    "readiness_providers",
    "readiness_provider_detail",
    "readiness_gates",
    "readiness_gates_detail",
    "readiness_controls",
    "readiness_controls_detail",
    "readiness_storage",
    "readiness_storage_detail",
    "readiness_source_free",
    "readiness_refreshing",
    "readiness_probing",
    "readiness_stale",
    "status_coordinator_configured",
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
    "blockers_action_external_dependency",
    "blockers_action_hardware",
    "blockers_action_authentication",
    "blockers_action_service",
    "blockers_action_platform",
    "blockers_action_prerequisite",
    "blockers_action_authority",
    "blockers_human_decisions",
    "blockers_human_action",
    "blockers_decision_scope",
    "blockers_decision_architecture",
    "blockers_decision_authority",
    "blockers_decision_branch",
    "blockers_decision_infrastructure",
    "blockers_decision_release",
    "blockers_decision_repeated",
    "blockers_no_holds",
    "blockers_no_matches",
    "blockers_deferred_tasks",
    "blockers_trigger",
    "blockers_trigger_provider_reset",
    "blockers_trigger_review_completion",
    "blockers_trigger_operator_resume",
    "blockers_trigger_head",
    "blockers_trigger_lease",
    "blockers_trigger_gate",
    "blockers_narrow_search",
    "blockers_manual_boundary",
    "navigation_shortcuts",
    "blockers_state",
    "changes_refresh_changes",
    "changes_refresh_runs",
    "changes_open_campaign",
    "changes_open_setup",
    "changes_no_project",
    "changes_no_campaign",
    "changes_status_needed",
    "changes_command_unavailable",
    "changes_unobserved",
    "changes_stale",
    "changes_title",
    "changes_delivery_title",
    "changes_publication_local",
    "changes_publication_draft",
    "changes_delivery_policy",
    "changes_range_title",
    "changes_status_withheld",
    "changes_never_started",
    "changes_status_unobserved",
    "changes_observation",
    "changes_loading",
    "changes_empty",
    "changes_truncated",
    "changes_commit",
    "changes_files_count",
    "changes_file_binary",
    "changes_paths_privacy",
    "records_loading",
    "records_unobserved",
    "records_stale",
    "records_title",
    "records_status_withheld",
    "records_empty",
    "records_truncated",
    "records_finding_limit",
    "records_run",
    "records_task_unknown",
    "records_candidate",
    "records_checkpoint_unavailable",
    "records_phases_truncated",
    "records_phases_empty",
    "records_phases",
    "activity_title",
    "activity_empty",
    "activity_notice",
    "activity_unrecorded",
    "blockers_deferral_satisfied",
    "blockers_deferral_unknown",
    "reports_title",
    "reports_no_project",
    "reports_open_campaign",
    "reports_open_setup",
    "reports_open_campaign_action",
    "reports_open_help",
    "reports_intro",
    "reports_sources_title",
    "reports_other_sources",
    "reports_open_changes",
    "reports_refresh_status",
    "reports_refresh_blockers",
    "reports_refresh_unavailable",
    "reports_status_observation",
    "reports_blocker_observation",
    "reports_status_loading",
    "reports_status_failed",
    "reports_status_unobserved",
    "reports_status_unknown",
    "reports_status_not_started",
    "reports_status_stale",
    "reports_blockers_loading",
    "reports_blockers_failed",
    "reports_blockers_unobserved",
    "reports_blockers_unknown",
    "reports_blockers_none",
    "reports_blockers_stale",
    "reports_source_warning",
    "reports_run_unknown",
    "reports_run_truncated",
    "reports_assembling",
    "reports_source_inspect_title",
    "reports_source_inspect_description",
    "reports_source_inspect",
    "reports_source_inspect_pending",
    "reports_source_inspect_failed",
    "reports_source_inspect_recovery",
    "reports_source_inspect_snapshot",
    "reports_source_inspect_unobserved",
    "reports_source_inspect_no_head",
    "reports_source_inspect_head",
    "reports_source_inspect_shortened",
    "reports_local_observation_title",
    "reports_assembly_failed",
    "reports_assembly_recovery",
    "reports_export_title",
    "reports_destination_guidance",
    "reports_destination",
    "reports_destination_hint",
    "reports_include_paths",
    "reports_overwrite",
    "reports_export",
    "reports_pending",
    "reports_refused",
    "reports_recovery",
    "reports_preview",
    "reports_preview_failed",
    "reports_preview_recovery",
    "reports_outcome_title",
    "reports_campaign_state",
    "reports_accepted",
    "reports_of",
    "reports_completed",
    "reports_hold_counts",
    "reports_commits",
    "reports_files",
    "reports_runs",
    "reports_delivery",
    "reports_blocker_title",
    "reports_blocker_absent",
    "reports_blocker_prefix",
    "reports_none",
    "reports_no_holds",
    "reports_blocked_prefix",
    "reports_deferred_prefix",
    "reports_until",
    "reports_human_prefix",
    "reports_clearance_boundary",
    "reports_not_observed",
    "reports_at_least",
    "reports_more_omitted",
    "work_plan_title",
    "work_plan_no_project",
    "work_plan_open_setup",
    "work_plan_open_help",
    "work_plan_source_unavailable",
    "work_plan_source_recovery",
    "work_plan_source_claim",
    "work_plan_items_shown",
    "work_plan_search",
    "work_plan_search_hint",
    "work_plan_ready_only",
    "work_plan_refresh_status",
    "work_plan_status_observation",
    "work_plan_status_loading",
    "work_plan_status_unavailable",
    "work_plan_status_failure_effect",
    "work_plan_status_stale",
    "work_plan_status_unrequested",
    "work_plan_status_absent",
    "work_plan_refresh_report",
    "work_plan_report_observation",
    "work_plan_report_loading",
    "work_plan_report_unavailable",
    "work_plan_report_failure_effect",
    "work_plan_report_stale",
    "work_plan_report_unrequested",
    "work_plan_report_absent",
    "work_plan_state_all",
    "work_plan_state_open",
    "work_plan_state_checked",
    "work_plan_kind_all",
    "work_plan_kind_subtasks",
    "work_plan_kind_tasks",
    "work_plan_kind_acceptance",
    "work_plan_sprint_header",
    "work_plan_story_header",
    "work_plan_checkbox_help",
    "work_plan_outcome_unknown",
    "work_plan_outcome_none",
    "work_plan_outcome_accepted",
    "work_plan_outcome_completed",
    "work_plan_outcome_human",
    "work_plan_outcome_blocked",
    "work_plan_outcome_deferred",
    "work_plan_outcome_active",
    "work_plan_outcome_for_item",
    "work_plan_outcome_label",
    "work_plan_outcome_hover_empty",
    "work_plan_coordinator_unknown",
    "work_plan_coordinator_heading",
    "work_plan_coordinator_none",
    "work_plan_row",
    "work_plan_kind_task",
    "work_plan_kind_subtask",
    "work_plan_kind_criterion",
    "work_plan_kind_gate",
    "work_plan_readiness_checked",
    "work_plan_readiness_ready",
    "work_plan_readiness_waiting",
    "work_plan_readiness_suffix",
    "work_plan_item_heading",
    "work_plan_detail_title",
    "work_plan_detail_source_checkbox",
    "work_plan_detail_source_open",
    "work_plan_detail_source_checked",
    "work_plan_detail_location",
    "work_plan_detail_location_value",
    "work_plan_detail_parent",
    "work_plan_detail_readiness",
    "work_plan_detail_readiness_na",
    "work_plan_detail_dependencies_none",
    "work_plan_detail_dependencies",
    "work_plan_detail_dependency_checked",
    "work_plan_detail_dependency_open",
    "work_plan_detail_dependency_unknown",
    "work_plan_detail_dependency_row",
    "work_plan_detail_dependents",
    "work_plan_detail_dependent_row",
    "work_plan_detail_copy_identifier",
    "work_plan_source_load",
    "work_plan_source_select_campaign",
    "work_plan_source_loading",
    "work_plan_source_failure",
    "work_plan_source_head",
    "work_plan_source_open",
    "work_plan_source_checked",
    "work_plan_source_excerpt",
    "work_plan_source_excerpt_truncated",
    "work_plan_source_criteria",
    "work_plan_source_criteria_none",
    "work_plan_source_criterion_state",
    "work_plan_source_criterion_title_truncated",
    "work_plan_source_criteria_truncated",
    "work_plan_runs_title",
    "work_plan_runs_command",
    "work_plan_runs_unavailable",
    "work_plan_runs_truncated",
    "work_plan_runs_none",
    "work_plan_runs_content_limit",
    "work_plan_runs_showing",
    "work_plan_runs_run",
    "work_plan_runs_candidate",
    "work_plan_runs_unknown_cause",
    "work_plan_runs_checkpoint_problem",
    "work_plan_runs_review_uncorroborated",
    "work_plan_runs_review_no_checkpoint",
    "work_plan_runs_review_absent",
    "work_plan_runs_review_verdict",
    "work_plan_runs_gate_uncorroborated",
    "work_plan_runs_gate_no_checkpoint",
    "work_plan_runs_gate_absent",
    "work_plan_runs_gate_summary",
    "work_plan_runs_journal_problem",
    "work_plan_runs_phases",
    "work_plan_runs_phases_truncated",
    "work_plan_runs_phases_showing",
    "work_plan_runs_phase",
    "campaign_title",
    "campaign_no_project",
    "campaign_no_selection",
    "campaign_authority_sha256",
    "campaign_repository_id",
    "campaign_initial_commit",
    "campaign_task_source_sha256",
    "campaign_execution",
    "campaign_accepted_outcome_ceiling",
    "campaign_publication",
    "campaign_specification",
    "campaign_select_campaign",
    "campaign_clear_campaign",
    "campaign_up",
    "campaign_binding_matches",
    "campaign_binding_drift_action",
    "campaign_durable_campaign_status",
    "campaign_refresh_campaign_status",
    "campaign_refreshing_retained",
    "campaign_status_unconfirmed",
    "campaign_status_loading",
    "campaign_status_not_started",
    "campaign_final_report_exists",
    "campaign_outcomes_title",
    "campaign_completed_and_reconciled",
    "campaign_blocked",
    "campaign_deferred",
    "campaign_awaiting_human_decision",
    "campaign_rejected_proposals_no_ceiling_use",
    "campaign_accepted_against_ceiling",
    "campaign_active_tasks",
    "campaign_no_unit_is_active",
    "campaign_phase",
    "campaign_actor",
    "campaign_model",
    "campaign_branch",
    "campaign_head",
    "campaign_current_task",
    "campaign_last_task",
    "campaign_watchdog",
    "campaign_reconciliation",
    "campaign_blocker_code",
    "campaign_elapsed",
    "campaign_updated",
    "campaign_blocked_tasks",
    "campaign_none",
    "campaign_deferred_tasks",
    "campaign_human_decisions_required",
    "campaign_utilization_against_limits",
    "campaign_provider_attempts",
    "campaign_malformed_report_repairs",
    "campaign_correction_rounds",
    "campaign_process_invocations",
    "campaign_output_bytes",
    "campaign_retained_state_bytes",
    "campaign_execution_time",
    "campaign_tokens_unknown",
    "campaign_roles_this_backend_reports",
    "campaign_owner_involvement",
    "campaign_involvement",
    "campaign_revocation_epoch",
    "campaign_expires_unix_ms",
    "campaign_decisions_recorded",
    "campaign_pending_owner_decisions",
    "campaign_owner_answers",
    "campaign_charter_digest",
    "campaign_no_mission",
    "campaign_mission_loading",
    "campaign_mission_authority_not_yet_observed",
    "campaign_open_setup",
    "campaign_open_help",
    "campaign_path_hint",
    "campaign_hide_browser",
    "campaign_browse",
    "campaign_spec_refused",
    "campaign_spec_refused_action",
    "campaign_status_stale",
    "campaign_named",
    "campaign_execution_parallel",
    "campaign_execution_serial",
    "campaign_binding_drift",
    "campaign_observation",
    "campaign_level_blocker",
    "campaign_final_summary",
    "campaign_active_line",
    "campaign_active_model",
    "campaign_active_pod",
    "campaign_elapsed_value",
    "campaign_updated_value",
    "campaign_used_of_limit",
    "campaign_used_time",
    "campaign_deferral_line",
    "campaign_reason_line",
    "campaign_roles_line",
    "campaign_mission_observation",
    "campaign_mission_stale",
    "campaign_mission_summary",
    "campaign_mission_revoked",
    "campaign_mission_expired",
    "campaign_mission_current",
    "campaign_mission_unverified",
    "campaign_mission_decisions",
    "campaign_mission_age",
    "campaign_mode_unavailable",
    "campaign_model_absent",
    "campaign_code_absent",
    "setup_title",
    "setup_intro",
    "setup_refused",
    "setup_correct_field",
    "setup_open_heading",
    "setup_workspace_directory",
    "setup_workspace_directory_hint",
    "setup_edit_configuration",
    "setup_authorization_heading",
    "setup_campaign_heading",
    "setup_export_heading",
    "setup_hide_browser",
    "setup_choose_repository",
    "setup_browser_up",
    "setup_use_target",
    "setup_config_previous_destination",
    "setup_config_pending",
    "setup_config_check_outcome",
    "setup_config_clear_notice",
    "setup_config_command_label",
    "setup_config_recovery_stdin",
    "setup_config_submission_stdin",
    "setup_config_title",
    "setup_config_file",
    "setup_config_target",
    "setup_config_task_source",
    "setup_config_default_branch",
    "setup_config_integration_branch",
    "setup_config_scratch_root",
    "setup_config_state_root",
    "setup_config_correction_limit",
    "setup_config_profiles",
    "setup_config_remove_profile",
    "setup_config_add_profile",
    "setup_config_gates",
    "setup_config_remove_gate",
    "setup_config_add_gate",
    "setup_config_publication",
    "setup_config_publication_local",
    "setup_config_publication_push",
    "setup_config_publication_draft",
    "setup_config_publication_hint",
    "setup_config_capabilities",
    "setup_config_capability_network",
    "setup_config_capability_push",
    "setup_config_capability_issues",
    "setup_config_capability_pull_requests",
    "setup_config_capability_task_merge",
    "setup_config_capability_destination_merge",
    "setup_config_parent_discovery",
    "setup_config_overwrite",
    "setup_config_apply",
    "setup_config_discard",
];

// Each named field is required exactly once in a template. A future translation may reorder
// fields, but cannot silently omit or invent an identity, count or authority state.
const TEMPLATE_FIELDS: &[(&str, &[&str])] = &[
    ("setup_config_previous_destination", &["destination"]),
    ("readiness_local_checks", &["passed", "failed", "unknown"]),
    ("readiness_observation", &["freshness", "age"]),
    ("readiness_report_state", &["state", "digest"]),
    (
        "readiness_repository_detail",
        &["id", "head", "clean", "branch", "checkout", "items", "open"],
    ),
    (
        "readiness_policy_detail",
        &["pods", "outcomes", "publication", "protected", "denied"],
    ),
    (
        "readiness_provider_detail",
        &["role", "verified", "probes", "authentication"],
    ),
    ("readiness_gates_detail", &["commands", "tiers"]),
    ("readiness_controls_detail", &["count", "guard"]),
    (
        "readiness_storage_detail",
        &["sufficient", "scratch", "state", "bytes"],
    ),
    ("changes_delivery_policy", &["policy"]),
    ("changes_observation", &["freshness", "age"]),
    ("changes_commit", &["commit", "subject", "timestamp"]),
    ("changes_files_count", &["count"]),
    ("records_run", &["run_id", "task_id"]),
    ("records_candidate", &["commit", "rounds"]),
    ("records_phases", &["phases"]),
    ("work_plan_source_claim", &["path"]),
    ("work_plan_items_shown", &["shown", "total"]),
    (
        "work_plan_status_observation",
        &["freshness", "age", "head"],
    ),
    ("work_plan_status_failure_effect", &["action"]),
    ("work_plan_report_observation", &["freshness", "age"]),
    ("work_plan_report_failure_effect", &["action"]),
    ("work_plan_sprint_header", &["id", "title"]),
    ("work_plan_story_header", &["id", "title"]),
    ("work_plan_outcome_for_item", &["badge", "id"]),
    ("work_plan_outcome_label", &["badge"]),
    (
        "work_plan_row",
        &["checkbox", "kind", "id", "title", "readiness"],
    ),
    ("work_plan_readiness_suffix", &["state"]),
    ("work_plan_item_heading", &["id"]),
    ("work_plan_detail_location_value", &["line", "digest"]),
    ("work_plan_detail_dependency_row", &["id", "state"]),
    ("work_plan_detail_dependent_row", &["id"]),
    ("work_plan_source_failure", &["code"]),
    ("work_plan_source_head", &["head", "state"]),
    ("work_plan_source_criterion_state", &["state"]),
    ("work_plan_runs_showing", &["shown", "total"]),
    ("work_plan_runs_run", &["run_id"]),
    ("work_plan_runs_candidate", &["commit", "rounds"]),
    ("work_plan_runs_checkpoint_problem", &["problem"]),
    ("work_plan_runs_review_verdict", &["verdict"]),
    ("work_plan_runs_gate_summary", &["count", "ids"]),
    ("work_plan_runs_journal_problem", &["problem"]),
    ("work_plan_runs_phases", &["count"]),
    (
        "work_plan_runs_phase",
        &["timestamp", "phase", "kind", "outcome"],
    ),
    ("campaign_named", &["campaign_id"]),
    ("campaign_execution_parallel", &["pods"]),
    ("campaign_binding_drift", &["reason"]),
    ("campaign_observation", &["freshness", "age"]),
    ("campaign_level_blocker", &["code"]),
    ("campaign_final_summary", &["commit", "accepted"]),
    (
        "campaign_active_line",
        &[
            "task",
            "state",
            "actor",
            "model",
            "round",
            "heartbeat",
            "pod",
        ],
    ),
    ("campaign_active_model", &["model"]),
    ("campaign_active_pod", &["pod"]),
    ("campaign_elapsed_value", &["seconds"]),
    ("campaign_updated_value", &["timestamp"]),
    ("campaign_used_of_limit", &["used", "limit"]),
    ("campaign_used_time", &["used", "limit"]),
    (
        "campaign_deferral_line",
        &["task", "reason", "trigger", "state"],
    ),
    ("campaign_reason_line", &["task", "reason"]),
    ("campaign_roles_line", &["code", "description"]),
    ("campaign_mission_observation", &["freshness", "age"]),
    (
        "campaign_mission_summary",
        &["id", "generation", "mode", "authority"],
    ),
    (
        "campaign_mission_decisions",
        &["recorded", "permitted", "held"],
    ),
    ("campaign_mission_age", &["age"]),
    ("campaign_mode_unavailable", &["label", "description"]),
];

enum Segment<'a> {
    Literal(&'a str),
    Field(&'a str),
}

fn segments(template: &str) -> Result<Vec<Segment<'_>>, String> {
    let mut parts = Vec::new();
    let mut cursor = 0;
    while cursor < template.len() {
        let tail = &template[cursor..];
        let Some(offset) = tail.find(['{', '}']) else {
            parts.push(Segment::Literal(tail));
            break;
        };
        let brace = cursor + offset;
        if brace > cursor {
            parts.push(Segment::Literal(&template[cursor..brace]));
        }
        if template.as_bytes()[brace] != b'{' {
            return Err("unmatched closing brace in message".to_owned());
        }
        let name_start = brace + 1;
        let Some(end_offset) = template[name_start..].find('}') else {
            return Err("unclosed message field".to_owned());
        };
        let end = name_start + end_offset;
        let name = &template[name_start..end];
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
        {
            return Err("invalid message field".to_owned());
        }
        parts.push(Segment::Field(name));
        cursor = end + 1;
    }
    Ok(parts)
}

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
        for (key, value) in &document.messages {
            let actual = segments(value)?
                .into_iter()
                .filter_map(|part| match part {
                    Segment::Field(name) => Some(name),
                    Segment::Literal(_) => None,
                })
                .collect::<Vec<_>>();
            let expected = TEMPLATE_FIELDS
                .iter()
                .find_map(|(candidate, fields)| (*candidate == key.as_str()).then_some(*fields))
                .unwrap_or(&[]);
            let actual_set: BTreeSet<_> = actual.iter().copied().collect();
            let expected_set: BTreeSet<_> = expected.iter().copied().collect();
            if actual.len() != actual_set.len() || actual_set != expected_set {
                return Err(format!(
                    "message {key} has missing, unknown or repeated fields"
                ));
            }
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

    pub(crate) fn format(&self, key: &str, values: &[(&str, &str)]) -> String {
        let expected = TEMPLATE_FIELDS
            .iter()
            .find_map(|(candidate, fields)| (*candidate == key).then_some(*fields))
            .unwrap_or_else(|| panic!("missing message template declaration: {key}"));
        assert_eq!(
            values.len(),
            expected.len(),
            "message arguments differ: {key}"
        );
        assert!(
            expected.iter().all(|name| {
                values
                    .iter()
                    .filter(|(candidate, _)| candidate == name)
                    .count()
                    == 1
            }),
            "message arguments differ: {key}"
        );
        let template = self.text(key);
        let mut rendered = String::with_capacity(template.len());
        for part in segments(template).expect("validated bundled message template") {
            match part {
                Segment::Literal(text) => rendered.push_str(text),
                Segment::Field(name) => rendered.push_str(
                    values
                        .iter()
                        .find_map(|(candidate, value)| (*candidate == name).then_some(*value))
                        .unwrap_or_else(|| panic!("missing value for message field {name}")),
                ),
            }
        }
        rendered
    }

    pub(crate) fn failure_title(&self, state: FailureState) -> &str {
        let key = match state {
            FailureState::ExecutableMissing => "failure_executable_missing",
            FailureState::PermissionDenied => "failure_permission_denied",
            FailureState::AuthenticationRequired => "failure_authentication_required",
            FailureState::ProviderUnavailable => "failure_provider_unavailable",
            FailureState::MalformedOutput => "failure_malformed_output",
            FailureState::IdentityMismatch => "failure_identity_mismatch",
            FailureState::UnsupportedSchema => "failure_unsupported_schema",
            FailureState::OutputTooLarge => "failure_output_too_large",
            FailureState::CoordinatorUnavailable => "failure_coordinator_unavailable",
            FailureState::CoordinatorTimedOut => "failure_coordinator_timed_out",
            FailureState::RequestFailed => "failure_request_failed",
        };
        self.text(key)
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

    #[test]
    fn readiness_template_field_contract() {
        let original =
            "readiness_report_state = \"Preflight state {state} - report sha256 {digest}\"";
        for replacement in [
            "readiness_report_state = \"Preflight state {state}\"",
            "readiness_report_state = \"Preflight state {state} - {other}\"",
            "readiness_report_state = \"{state} {digest} {digest}\"",
            "readiness_report_state = \"{state} {digest\"",
        ] {
            assert!(Catalogue::parse(&SOURCE.replace(original, replacement)).is_err());
        }
        let rendered = english().format(
            "readiness_report_state",
            &[("digest", "a"), ("state", "ready")],
        );
        assert_eq!(rendered, "Preflight state ready - report sha256 a");
    }

    #[test]
    fn dynamic_messages_reject_dropped_or_forged_identity_fields() {
        let original = "campaign_named = \"Campaign {campaign_id}\"";
        for replacement in [
            "campaign_named = \"Campaign\"",
            "campaign_named = \"Campaign {other}\"",
            "campaign_named = \"Campaign {campaign_id} {campaign_id}\"",
            "campaign_named = \"Campaign {campaign_id\"",
            "campaign_named = \"Campaign {campaign_id}}\"",
        ] {
            assert!(Catalogue::parse(&SOURCE.replace(original, replacement)).is_err());
        }
        assert!(
            Catalogue::parse(&SOURCE.replace(
                "help_title = \"Help and About\"",
                "help_title = \"{campaign_id}\""
            ))
            .is_err()
        );
    }

    #[test]
    fn dynamic_messages_allow_reordered_fields_and_do_not_reparse_backend_text() {
        let source = SOURCE.replace(
            "campaign_observation = \"Observation: {freshness} ({age})\"",
            "campaign_observation = \"{age}: {freshness}\"",
        );
        let catalogue = Catalogue::parse(&source).unwrap();
        assert_eq!(
            catalogue.format(
                "campaign_observation",
                &[("freshness", "stale"), ("age", "10 s")],
            ),
            "10 s: stale"
        );
        let value = "{age}<script>alert(1)</script>";
        assert_eq!(
            english().format("campaign_named", &[("campaign_id", value)]),
            format!("Campaign {value}")
        );
        let pseudo = english().pseudo(true);
        assert!(
            pseudo
                .format("campaign_named", &[("campaign_id", "campaign-1")])
                .contains("campaign-1")
        );
    }

    #[test]
    fn work_plan_templates_preserve_observation_fields_and_inert_values() {
        let original = "work_plan_status_observation = \"Coordinator outcomes: {freshness} ({age}); campaign-head source {head}\"";
        for replacement in [
            "work_plan_status_observation = \"Coordinator outcomes: {freshness} ({age})\"",
            "work_plan_status_observation = \"{freshness} {age} {head} {head}\"",
            "work_plan_status_observation = \"{freshness} {age} {other}\"",
        ] {
            assert!(Catalogue::parse(&SOURCE.replace(original, replacement)).is_err());
        }
        let reordered = SOURCE.replace(
            original,
            "work_plan_status_observation = \"{head}; {age}; {freshness}\"",
        );
        let catalogue = Catalogue::parse(&reordered).unwrap();
        assert_eq!(
            catalogue.format(
                "work_plan_status_observation",
                &[
                    ("freshness", "{head}<script>"),
                    ("age", "10s ago"),
                    ("head", "live"),
                ],
            ),
            "live; 10s ago; {head}<script>"
        );
    }

    #[test]
    fn work_plan_row_template_rejects_lost_identity_and_keeps_source_inert() {
        let original = "work_plan_row = \"{checkbox} {kind} {id} {title}{readiness}\"";
        for replacement in [
            "work_plan_row = \"{checkbox} {kind} {title}{readiness}\"",
            "work_plan_row = \"{checkbox} {kind} {id} {id} {title}{readiness}\"",
            "work_plan_row = \"{checkbox} {kind} {id} {title}{unknown}\"",
        ] {
            assert!(Catalogue::parse(&SOURCE.replace(original, replacement)).is_err());
        }
        let catalogue = Catalogue::parse(SOURCE).unwrap();
        assert_eq!(
            catalogue.format(
                "work_plan_row",
                &[
                    ("checkbox", "[ ]"),
                    ("kind", "Sub-task"),
                    ("id", "1.1.1.2"),
                    ("title", "{id}<script>"),
                    ("readiness", " - dependency-ready"),
                ],
            ),
            "[ ] Sub-task 1.1.1.2 {id}<script> - dependency-ready"
        );
    }

    #[test]
    fn source_detail_template_requires_head_state_and_literal_error_code() {
        let original = "work_plan_source_head = \"Source at campaign head {head}…; checkbox is {state}, not a verified outcome.\"";
        for replacement in [
            "work_plan_source_head = \"Source at campaign head {head}…; source checkbox is unknown.\"",
            "work_plan_source_head = \"Source at campaign head {head}…; checkbox is {state}; {state}.\"",
            "work_plan_source_head = \"Source at campaign head {head}…; checkbox is {unexpected}.\"",
        ] {
            assert!(Catalogue::parse(&SOURCE.replace(original, replacement)).is_err());
        }
        let catalogue = Catalogue::parse(SOURCE).unwrap();
        assert_eq!(
            catalogue.format("work_plan_source_failure", &[("code", "{state}<script>")],),
            "Source detail unavailable: {state}<script>. Refresh the campaign and try again."
        );
    }

    #[test]
    fn task_run_templates_require_exact_evidence_identity_and_counts() {
        let run = "work_plan_runs_run = \"Run {run_id}\"";
        for replacement in [
            "work_plan_runs_run = \"Run\"",
            "work_plan_runs_run = \"Run {run_id} {run_id}\"",
            "work_plan_runs_run = \"Run {other}\"",
        ] {
            assert!(Catalogue::parse(&SOURCE.replace(run, replacement)).is_err());
        }
        let candidate = "work_plan_runs_candidate = \"Candidate commit {commit} · {rounds} correction round(s)\"";
        assert!(
            Catalogue::parse(&SOURCE.replace(
                candidate,
                "work_plan_runs_candidate = \"Candidate commit {commit}\""
            ))
            .is_err()
        );
        let catalogue = Catalogue::parse(SOURCE).unwrap();
        assert_eq!(
            catalogue.format("work_plan_runs_run", &[("run_id", "{other}<script>")]),
            "Run {other}<script>"
        );
    }
}
