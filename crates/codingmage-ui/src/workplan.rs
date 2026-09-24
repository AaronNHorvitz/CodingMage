//! Read-only work plan built from the strict task parser.
//!
//! The plan view shows the exact source checkbox state, dependencies and source anchors. It
//! never edits the task source. Later screens overlay coordinator observations on top of this
//! view without changing the source state it presents.

use std::collections::{BTreeMap, BTreeSet};

use codingmage_plan::{CheckState, PlanItem, PlanItemKind, TaskPlan};

/// Source checkbox filter.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum StateFilter {
    /// Every item.
    #[default]
    All,
    /// Only open checkboxes.
    Open,
    /// Only checked checkboxes.
    Checked,
}

impl StateFilter {
    /// All filters in display order.
    pub const ALL: [Self; 3] = [Self::All, Self::Open, Self::Checked];

    /// Display label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Open => "Open",
            Self::Checked => "Checked in source",
        }
    }
}

/// Item kind filter.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum KindFilter {
    /// Every kind.
    #[default]
    All,
    /// Sub-tasks only.
    SubTasks,
    /// Tasks only.
    Tasks,
    /// Acceptance criteria and gates.
    Acceptance,
}

impl KindFilter {
    /// All filters in display order.
    pub const ALL: [Self; 4] = [Self::All, Self::SubTasks, Self::Tasks, Self::Acceptance];

    /// Display label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "All kinds",
            Self::SubTasks => "Sub-tasks",
            Self::Tasks => "Tasks",
            Self::Acceptance => "Acceptance and gates",
        }
    }

    fn accepts(self, kind: PlanItemKind) -> bool {
        match self {
            Self::All => true,
            Self::SubTasks => kind == PlanItemKind::SubTask,
            Self::Tasks => kind == PlanItemKind::Task,
            Self::Acceptance => {
                matches!(kind, PlanItemKind::AcceptanceCriterion | PlanItemKind::Gate)
            }
        }
    }
}

/// Filter state for the plan view.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PlanFilter {
    /// Case-insensitive substring matched against id and title.
    pub query: String,
    /// Checkbox filter.
    pub state: StateFilter,
    /// Kind filter.
    pub kind: KindFilter,
    /// Only dependency-ready open sub-tasks.
    pub ready_only: bool,
}

/// Readiness of one item derived from the source alone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceReadiness {
    /// Checked in the source.
    Checked,
    /// Open and every dependency is checked.
    Ready,
    /// Open with at least one open or unknown dependency.
    Waiting,
    /// Not a sub-task; readiness is not computed.
    NotApplicable,
}

impl SourceReadiness {
    /// Display label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Checked => "checked in source",
            Self::Ready => "dependency-ready",
            Self::Waiting => "waiting on dependencies",
            Self::NotApplicable => "",
        }
    }
}

/// One dependency of an item with its resolved state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DependencyView {
    /// Dependency identifier as written in the source.
    pub id: String,
    /// Resolved source state, `None` when the identifier is unknown.
    pub state: Option<CheckState>,
}

/// One row of the filtered plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanRow {
    /// Item identifier.
    pub id: String,
    /// Item kind.
    pub kind: PlanItemKind,
    /// Item title.
    pub title: String,
    /// Source checkbox.
    pub state: CheckState,
    /// Parent identifier.
    pub parent_id: String,
    /// Sprint identifier this row belongs to.
    pub sprint_id: String,
    /// Story identifier this row belongs to, when any.
    pub story_id: Option<String>,
    /// Source line.
    pub line: usize,
    /// Source line digest.
    pub line_sha256: String,
    /// Resolved dependencies.
    pub dependencies: Vec<DependencyView>,
    /// Readiness derived from the source.
    pub readiness: SourceReadiness,
}

/// Counts shown in the overview.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PlanCounts {
    /// Total items.
    pub items: usize,
    /// Open sub-tasks.
    pub open_subtasks: usize,
    /// Checked sub-tasks.
    pub checked_subtasks: usize,
    /// Dependency-ready open sub-tasks.
    pub ready_subtasks: usize,
    /// Open acceptance criteria and gates.
    pub open_acceptance: usize,
}

/// Index over a parsed plan.
#[derive(Clone, Debug)]
pub struct PlanIndex {
    rows: Vec<PlanRow>,
    counts: PlanCounts,
    sprint_titles: BTreeMap<String, String>,
    story_titles: BTreeMap<String, String>,
}

impl PlanIndex {
    /// Builds the index from a parsed plan.
    #[must_use]
    pub fn new(plan: &TaskPlan) -> Self {
        let by_id: BTreeMap<&str, &PlanItem> = plan
            .items
            .iter()
            .map(|item| (item.id.as_str(), item))
            .collect();
        let story_sprint: BTreeMap<&str, &str> = plan
            .stories
            .iter()
            .map(|story| (story.id.as_str(), story.sprint_id.as_str()))
            .collect();
        let sprint_ids: BTreeSet<&str> = plan
            .sprints
            .iter()
            .map(|sprint| sprint.id.as_str())
            .collect();
        let mut rows = Vec::with_capacity(plan.items.len());
        let mut counts = PlanCounts {
            items: plan.items.len(),
            ..PlanCounts::default()
        };
        for item in &plan.items {
            let (sprint_id, story_id) = locate(item, &by_id, &story_sprint, &sprint_ids);
            let dependencies = item
                .dependencies
                .iter()
                .map(|id| DependencyView {
                    id: id.clone(),
                    state: by_id.get(id.as_str()).map(|dependency| dependency.state),
                })
                .collect::<Vec<_>>();
            let readiness = if item.kind != PlanItemKind::SubTask {
                SourceReadiness::NotApplicable
            } else if item.state == CheckState::Checked {
                SourceReadiness::Checked
            } else if dependencies
                .iter()
                .all(|dependency| dependency.state == Some(CheckState::Checked))
            {
                SourceReadiness::Ready
            } else {
                SourceReadiness::Waiting
            };
            match (item.kind, item.state) {
                (PlanItemKind::SubTask, CheckState::Open) => counts.open_subtasks += 1,
                (PlanItemKind::SubTask, CheckState::Checked) => counts.checked_subtasks += 1,
                (PlanItemKind::AcceptanceCriterion | PlanItemKind::Gate, CheckState::Open) => {
                    counts.open_acceptance += 1;
                }
                _ => {}
            }
            if readiness == SourceReadiness::Ready {
                counts.ready_subtasks += 1;
            }
            rows.push(PlanRow {
                id: item.id.clone(),
                kind: item.kind,
                title: item.title.clone(),
                state: item.state,
                parent_id: item.parent_id.clone(),
                sprint_id,
                story_id,
                line: item.anchor.line,
                line_sha256: item.anchor.line_sha256.clone(),
                dependencies,
                readiness,
            });
        }
        Self {
            rows,
            counts,
            sprint_titles: plan
                .sprints
                .iter()
                .map(|sprint| (sprint.id.clone(), sprint.title.clone()))
                .collect(),
            story_titles: plan
                .stories
                .iter()
                .map(|story| (story.id.clone(), story.title.clone()))
                .collect(),
        }
    }

    /// All rows in source order.
    #[must_use]
    pub fn rows(&self) -> &[PlanRow] {
        &self.rows
    }

    /// Overview counts.
    #[must_use]
    pub const fn counts(&self) -> &PlanCounts {
        &self.counts
    }

    /// Sprint title by identifier.
    #[must_use]
    pub fn sprint_title(&self, id: &str) -> Option<&str> {
        self.sprint_titles.get(id).map(String::as_str)
    }

    /// Story title by identifier.
    #[must_use]
    pub fn story_title(&self, id: &str) -> Option<&str> {
        self.story_titles.get(id).map(String::as_str)
    }

    /// Rows matching a filter, in source order.
    #[must_use]
    pub fn filtered(&self, filter: &PlanFilter) -> Vec<&PlanRow> {
        let query = filter.query.trim().to_lowercase();
        self.rows
            .iter()
            .filter(|row| filter.kind.accepts(row.kind))
            .filter(|row| match filter.state {
                StateFilter::All => true,
                StateFilter::Open => row.state == CheckState::Open,
                StateFilter::Checked => row.state == CheckState::Checked,
            })
            .filter(|row| !filter.ready_only || row.readiness == SourceReadiness::Ready)
            .filter(|row| {
                query.is_empty()
                    || row.id.to_lowercase().contains(&query)
                    || row.title.to_lowercase().contains(&query)
            })
            .collect()
    }

    /// Items that depend on the given identifier.
    #[must_use]
    pub fn dependents(&self, id: &str) -> Vec<&PlanRow> {
        self.rows
            .iter()
            .filter(|row| {
                row.dependencies
                    .iter()
                    .any(|dependency| dependency.id == id)
            })
            .collect()
    }
}

fn locate(
    item: &PlanItem,
    by_id: &BTreeMap<&str, &PlanItem>,
    story_sprint: &BTreeMap<&str, &str>,
    sprint_ids: &BTreeSet<&str>,
) -> (String, Option<String>) {
    let mut current = item.parent_id.as_str();
    for _ in 0..16 {
        if let Some(sprint) = story_sprint.get(current) {
            return ((*sprint).to_owned(), Some(current.to_owned()));
        }
        if sprint_ids.contains(current) {
            return (current.to_owned(), None);
        }
        match by_id.get(current) {
            Some(parent) => current = parent.parent_id.as_str(),
            None => break,
        }
    }
    (item.parent_id.clone(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> TaskPlan {
        TaskPlan::parse(
            b"# Tasks\n\n## Sprint 1 - Start\n\n**Sprint goal:** Start.\n\n### Story 1.1 - First\n\n- [ ] **Task 1.1.1 - Work**\n  - [x] **Sub-task 1.1.1.1:** Complete the first fixture operation.\n  - [ ] **Sub-task 1.1.1.2:** Complete the second fixture operation.\n    <!-- depends-on: 1.1.1.1 -->\n  - [ ] **Sub-task 1.1.1.3:** Complete the third fixture operation.\n    <!-- depends-on: 1.1.1.2 -->\n\n- [ ] **AC 1.1:** Given the fixture, when it runs, then it passes.\n",
        )
        .unwrap()
    }

    #[test]
    fn index_resolves_dependencies_readiness_and_counts() {
        let index = PlanIndex::new(&plan());
        let counts = index.counts();
        assert_eq!(counts.open_subtasks, 2);
        assert_eq!(counts.checked_subtasks, 1);
        assert_eq!(counts.ready_subtasks, 1);
        assert_eq!(counts.open_acceptance, 1);
        let second = index.rows().iter().find(|row| row.id == "1.1.1.2").unwrap();
        assert_eq!(second.readiness, SourceReadiness::Ready);
        assert_eq!(second.dependencies[0].state, Some(CheckState::Checked));
        assert_eq!(second.sprint_id, "1");
        assert_eq!(second.story_id.as_deref(), Some("1.1"));
        let third = index.rows().iter().find(|row| row.id == "1.1.1.3").unwrap();
        assert_eq!(third.readiness, SourceReadiness::Waiting);
        assert_eq!(index.dependents("1.1.1.1").len(), 1);
    }

    #[test]
    fn filters_combine_query_state_kind_and_readiness() {
        let index = PlanIndex::new(&plan());
        let ready = index.filtered(&PlanFilter {
            ready_only: true,
            ..PlanFilter::default()
        });
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "1.1.1.2");
        let checked = index.filtered(&PlanFilter {
            state: StateFilter::Checked,
            ..PlanFilter::default()
        });
        assert_eq!(checked.len(), 1);
        let acceptance = index.filtered(&PlanFilter {
            kind: KindFilter::Acceptance,
            ..PlanFilter::default()
        });
        assert_eq!(acceptance[0].id, "1.1");
        let query = index.filtered(&PlanFilter {
            query: "THIRD".to_owned(),
            ..PlanFilter::default()
        });
        assert_eq!(query.len(), 1);
    }
}
