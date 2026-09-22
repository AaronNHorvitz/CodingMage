//! Read-only work plan over the real task parser: search, filters, dependencies and anchors.

mod common;

use std::{fs, time::Duration};

use codingmage_ui::{
    Screen,
    backend::CoordinatorBinary,
    workplan::{KindFilter, PlanFilter, SourceReadiness, StateFilter},
};
use common::{Fixture, coordinator_binary, git, harness, settle, tree_digest};
use egui_kittest::kittest::Queryable as _;

const PLAN: &str = "# Tasks\n\n## Sprint 1 - Start\n\n**Sprint goal:** Start safely.\n\n### Story 1.1 - First\n\n- [ ] **Task 1.1.1 - Work**\n  - [x] **Sub-task 1.1.1.1:** Complete the first fixture operation safely.\n  - [ ] **Sub-task 1.1.1.2:** Complete the second fixture operation safely.\n    <!-- depends-on: 1.1.1.1 -->\n  - [ ] **Sub-task 1.1.1.3:** Complete the third fixture operation safely.\n    <!-- depends-on: 1.1.1.2 -->\n\n- [ ] **AC 1.1:** Given the fixture, when it runs, then the value changes.\n";

fn open_fixture(label: &str) -> (Fixture, egui_kittest::Harness<'static, codingmage_ui::App>) {
    let fixture = Fixture::new(label, 3);
    fs::write(fixture.target.join("TASKS.md"), PLAN).unwrap();
    git(&fixture.target, &["add", "TASKS.md"]);
    git(&fixture.target, &["commit", "-q", "-m", "plan"]);
    let binary = CoordinatorBinary::at(&coordinator_binary());
    let mut harness = harness(binary, [1100.0, 720.0]);
    let config = fixture.config.clone();
    harness.state_mut().open_project(&config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().value.is_some()
    }));
    harness.state_mut().select_screen(Screen::WorkPlan);
    harness.run_steps(2);
    (fixture, harness)
}

#[test]
fn work_plan_shows_source_states_dependencies_and_anchors_without_editing() {
    let (fixture, mut harness) = open_fixture("workplan");
    let before = tree_digest(&fixture.target);
    let head = fixture.head();
    harness.get_by_label_contains("[x] Sub-task 1.1.1.1");
    harness.get_by_label_contains(
        "[ ] Sub-task 1.1.1.2 Complete the second fixture operation safely. - dependency-ready",
    );
    harness.get_by_label_contains("[ ] Sub-task 1.1.1.3 Complete the third fixture operation safely. - waiting on dependencies");
    harness.get_by_label_contains("[ ] AC 1.1");
    harness.get_by_label_contains("5 of 5 items shown");
    harness
        .get_by_label_contains("[ ] Sub-task 1.1.1.3")
        .click();
    harness.run_steps(2);
    assert_eq!(harness.state().selected_item(), Some("1.1.1.3"));
    harness.get_by_label_contains("Item 1.1.1.3");
    harness.get_by_label_contains("1.1.1.2 - open");
    harness.get_by_label_contains("line 13");
    harness
        .get_by_label_contains("[ ] Sub-task 1.1.1.2")
        .click();
    harness.run_steps(2);
    harness.get_by_label_contains("1.1.1.1 - checked");
    harness.get_by_label_contains("Depended on by");
    assert_eq!(tree_digest(&fixture.target), before);
    assert_eq!(fixture.head(), head);
    assert_eq!(
        fs::read_to_string(fixture.target.join("TASKS.md")).unwrap(),
        PLAN
    );
}

#[test]
fn filters_and_search_narrow_the_plan() {
    let (_fixture, mut harness) = open_fixture("filters");
    harness.state_mut().set_plan_filter(PlanFilter {
        ready_only: true,
        ..PlanFilter::default()
    });
    harness.run_steps(2);
    harness.get_by_label_contains("1 of 5 items shown");
    harness.get_by_label_contains("[ ] Sub-task 1.1.1.2");
    assert!(
        harness
            .query_by_label_contains("[ ] Sub-task 1.1.1.3")
            .is_none()
    );
    harness.state_mut().set_plan_filter(PlanFilter {
        state: StateFilter::Checked,
        ..PlanFilter::default()
    });
    harness.run_steps(2);
    harness.get_by_label_contains("1 of 5 items shown");
    harness.get_by_label_contains("[x] Sub-task 1.1.1.1");
    harness.state_mut().set_plan_filter(PlanFilter {
        kind: KindFilter::Acceptance,
        ..PlanFilter::default()
    });
    harness.run_steps(2);
    harness.get_by_label_contains("[ ] AC 1.1");
    harness.state_mut().set_plan_filter(PlanFilter {
        query: "THIRD".to_owned(),
        ..PlanFilter::default()
    });
    harness.run_steps(2);
    harness.get_by_label_contains("1 of 5 items shown");
    let index = harness.state().plan_index().unwrap();
    assert_eq!(index.counts().ready_subtasks, 1);
    assert_eq!(
        index
            .rows()
            .iter()
            .find(|row| row.id == "1.1.1.3")
            .unwrap()
            .readiness,
        SourceReadiness::Waiting
    );
}

#[test]
fn malformed_task_source_is_a_visible_failure_and_overview_still_works() {
    let fixture = Fixture::new("malformed-plan", 3);
    fs::write(
        fixture.target.join("TASKS.md"),
        "# Tasks\n\n- [ ] **Sub-task 9.9.9.9:** Orphan without a sprint.\n",
    )
    .unwrap();
    git(&fixture.target, &["add", "TASKS.md"]);
    git(&fixture.target, &["commit", "-q", "-m", "broken plan"]);
    let binary = CoordinatorBinary::at(&coordinator_binary());
    let mut harness = harness(binary, [1100.0, 720.0]);
    let config = fixture.config.clone();
    harness.state_mut().open_project(&config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().value.is_some() || app.diagnosis().last_error.is_some()
    }));
    assert!(harness.state().project().is_some());
    assert!(harness.state().plan_index().is_none());
    harness.state_mut().select_screen(Screen::WorkPlan);
    harness.run_steps(2);
    harness.get_by_label_contains("Task source unavailable");
    harness.state_mut().select_screen(Screen::Overview);
    harness.run_steps(2);
    harness.get_by_label_contains("Task source unavailable");
}

#[test]
fn browser_lists_directories_and_opens_a_configuration() {
    let fixture = Fixture::new("browser", 3);
    let binary = CoordinatorBinary::at(&coordinator_binary());
    let mut harness = harness(binary, [1100.0, 720.0]);
    harness.state_mut().select_screen(Screen::Setup);
    harness.run_steps(2);
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Browse")
        .click();
    harness.run_steps(2);
    harness.get_by_role_and_label(egui::accesskit::Role::Button, "Up");
    let browser = codingmage_ui::browser::Browser::new(&fixture.root.join("config"), vec!["toml"]);
    assert_eq!(browser.entries.len(), 1);
    assert!(browser.entries[0].path.ends_with("codingmage.toml"));
    assert!(harness.state().project().is_none());
}
