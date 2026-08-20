//! Strict Markdown task-plan parsing, selection, and bounded work packets.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::{Path, PathBuf},
};

use codingmage_contracts::{RepositoryId, RunId, TaskId, WorktreeId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const MAX_PLAN_BYTES: usize = 8 * 1024 * 1024;
const MAX_ITEMS: usize = 20_000;
const MAX_PACKET_BYTES: usize = 2 * 1024 * 1024;

/// Checked state read from one literal Markdown checkbox.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckState {
    /// Work remains open.
    Open,
    /// Canonical source claims completion; evidence is verified elsewhere.
    Checked,
}

/// Kind of one parsed plan node.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanItemKind {
    /// Sprint-level task.
    Task,
    /// Bounded task child.
    SubTask,
    /// Story acceptance criterion.
    AcceptanceCriterion,
    /// Sprint gate.
    Gate,
}

/// Immutable source position and line hash.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceAnchor {
    /// One-based line number.
    pub line: usize,
    /// SHA-256 of the exact source line, including its line ending.
    pub line_sha256: String,
}

/// One strict checklist item.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlanItem {
    /// Stable dotted source identifier.
    pub id: String,
    /// Node kind.
    pub kind: PlanItemKind,
    /// Exact normalized title after the identifier.
    pub title: String,
    /// Checkbox state.
    pub state: CheckState,
    /// Parent story, task, or sprint identifier.
    pub parent_id: String,
    /// Explicit dependency identifiers.
    pub dependencies: Vec<String>,
    /// Immutable source anchor.
    pub anchor: SourceAnchor,
}

/// Parsed story and its sprint association.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlanStory {
    /// Story identifier.
    pub id: String,
    /// Parent sprint identifier.
    pub sprint_id: String,
    /// Story title.
    pub title: String,
    /// Source anchor.
    pub anchor: SourceAnchor,
}

/// Parsed sprint and goal.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlanSprint {
    /// Sprint identifier.
    pub id: String,
    /// Sprint title.
    pub title: String,
    /// Required sprint goal.
    pub goal: String,
    /// Source anchor.
    pub anchor: SourceAnchor,
}

/// Immutable parse of one canonical task source.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskPlan {
    /// Schema version.
    pub version: u16,
    /// SHA-256 of all exact source bytes.
    pub source_sha256: String,
    /// Ordered sprints.
    pub sprints: Vec<PlanSprint>,
    /// Ordered stories.
    pub stories: Vec<PlanStory>,
    /// Ordered checklist items.
    pub items: Vec<PlanItem>,
}

impl TaskPlan {
    /// Parses the `CodingMage` Markdown checklist grammar without modifying source bytes.
    ///
    /// Supported dependency metadata is an immediately following comment such as
    /// `<!-- depends-on: 1.2.3.4, 2.1.1.1 -->`.
    ///
    /// # Errors
    ///
    /// Returns [`PlanError`] for malformed hierarchy, duplicate IDs, conflicting checkbox state,
    /// ambiguous dependencies, missing sprint goals, or excessive input.
    #[allow(clippy::too_many_lines)]
    pub fn parse(source: &[u8]) -> Result<Self, PlanError> {
        if source.is_empty() || source.len() > MAX_PLAN_BYTES {
            return Err(PlanError::InvalidSource);
        }
        let text = std::str::from_utf8(source).map_err(|_| PlanError::InvalidSource)?;
        let lines: Vec<&str> = text.split_inclusive('\n').collect();
        let mut sprints: Vec<PlanSprint> = Vec::new();
        let mut stories = Vec::new();
        let mut items: Vec<PlanItem> = Vec::new();
        let mut ids = BTreeSet::new();
        let mut item_id_counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut current_sprint: Option<String> = None;
        let mut current_story: Option<String> = None;
        let mut current_task: Option<String> = None;
        let mut waiting_for_goal: Option<usize> = None;

        for (index, exact) in lines.iter().enumerate() {
            let line_number = index + 1;
            let line = exact.trim_end_matches(['\r', '\n']);
            if let Some(rest) = sprint_heading(line) {
                let (id, title) = split_heading(rest)?;
                insert_id(&mut ids, "sprint", &id)?;
                sprints.push(PlanSprint {
                    id: id.clone(),
                    title,
                    goal: String::new(),
                    anchor: anchor(line_number, exact),
                });
                current_sprint = Some(id);
                current_story = None;
                current_task = None;
                waiting_for_goal = Some(sprints.len() - 1);
                continue;
            }
            if line.starts_with("## ") {
                current_sprint = None;
                current_story = None;
                current_task = None;
                waiting_for_goal = None;
                continue;
            }
            if let Some(goal) = line.strip_prefix("**Sprint goal:** ") {
                let position = waiting_for_goal
                    .take()
                    .ok_or(PlanError::MalformedHierarchy)?;
                if goal.trim().is_empty() {
                    return Err(PlanError::InvalidSource);
                }
                goal.trim().clone_into(&mut sprints[position].goal);
                continue;
            }
            if let Some(rest) = story_heading(line) {
                let sprint = current_sprint
                    .clone()
                    .ok_or_else(|| missing_parent(line_number, "story"))?;
                if waiting_for_goal.is_some() {
                    return Err(PlanError::MissingGoal);
                }
                let (id, title) = split_heading(rest)?;
                if dotted_parent(&id) != Some(sprint.as_str()) {
                    return Err(PlanError::MalformedHierarchy);
                }
                insert_id(&mut ids, "story", &id)?;
                stories.push(PlanStory {
                    id: id.clone(),
                    sprint_id: sprint,
                    title,
                    anchor: anchor(line_number, exact),
                });
                current_story = Some(id);
                current_task = None;
                continue;
            }
            if let Some((state, label)) = parse_checkbox(line) {
                let (kind, prefix) = if label.starts_with("Task ") {
                    (PlanItemKind::Task, "Task ")
                } else if label.starts_with("Sub-task ") {
                    (PlanItemKind::SubTask, "Sub-task ")
                } else if label.starts_with("Story AC ") {
                    (PlanItemKind::AcceptanceCriterion, "Story AC ")
                } else if label.starts_with("Sprint AC ") {
                    (PlanItemKind::AcceptanceCriterion, "Sprint AC ")
                } else if label.starts_with("AC ") {
                    (PlanItemKind::AcceptanceCriterion, "AC ")
                } else if label.starts_with("Gate ") {
                    (PlanItemKind::Gate, "Gate ")
                } else {
                    if current_sprint.is_none() {
                        continue;
                    }
                    return Err(PlanError::InvalidSource);
                };
                let (id, title) = split_item(&label[prefix.len()..])?;
                let namespace = match kind {
                    PlanItemKind::Task => "task",
                    PlanItemKind::SubTask => "subtask",
                    PlanItemKind::AcceptanceCriterion => "acceptance",
                    PlanItemKind::Gate => "gate",
                };
                insert_id(&mut ids, namespace, &id)?;
                *item_id_counts.entry(id.clone()).or_default() += 1;
                let parent_id = match kind {
                    PlanItemKind::Task => {
                        let story = current_story
                            .clone()
                            .ok_or_else(|| missing_parent(line_number, "task"))?;
                        if dotted_parent(&id) != Some(story.as_str()) {
                            return Err(PlanError::MalformedHierarchy);
                        }
                        current_task = Some(id.clone());
                        story
                    }
                    PlanItemKind::AcceptanceCriterion => {
                        let story = if stories.iter().any(|candidate| candidate.id == id) {
                            id.as_str()
                        } else {
                            dotted_parent(&id)
                                .ok_or_else(|| missing_parent(line_number, "acceptance"))?
                        };
                        if stories.iter().any(|candidate| candidate.id == story)
                            || current_sprint.as_deref() == Some(story)
                        {
                            story.to_owned()
                        } else {
                            return Err(missing_parent(line_number, "acceptance-parent"));
                        }
                    }
                    PlanItemKind::SubTask => {
                        let task = current_task
                            .clone()
                            .ok_or_else(|| missing_parent(line_number, "subtask"))?;
                        if dotted_parent(&id) != Some(task.as_str()) {
                            return Err(PlanError::MalformedHierarchy);
                        }
                        task
                    }
                    PlanItemKind::Gate => current_sprint
                        .clone()
                        .ok_or_else(|| missing_parent(line_number, "gate"))?,
                };
                items.push(PlanItem {
                    id,
                    kind,
                    title,
                    state,
                    parent_id,
                    dependencies: Vec::new(),
                    anchor: anchor(line_number, exact),
                });
                if items.len() > MAX_ITEMS {
                    return Err(PlanError::InvalidSource);
                }
                continue;
            }
            if let Some(metadata) = line.trim().strip_prefix("<!-- depends-on:") {
                let metadata = metadata
                    .strip_suffix("-->")
                    .ok_or(PlanError::InvalidDependency)?;
                let item = items.last_mut().ok_or(PlanError::InvalidDependency)?;
                if !item.dependencies.is_empty() {
                    return Err(PlanError::InvalidDependency);
                }
                item.dependencies = metadata
                    .split(',')
                    .map(str::trim)
                    .map(str::to_owned)
                    .collect();
                if item.dependencies.is_empty()
                    || item.dependencies.iter().any(String::is_empty)
                    || item.dependencies.iter().collect::<BTreeSet<_>>().len()
                        != item.dependencies.len()
                {
                    return Err(PlanError::InvalidDependency);
                }
            }
        }
        if waiting_for_goal.is_some()
            || sprints.is_empty()
            || stories.is_empty()
            || items.is_empty()
        {
            return Err(PlanError::MissingGoal);
        }
        validate_dependencies(&items, &item_id_counts)?;
        validate_states(&items)?;
        Ok(Self {
            version: 1,
            source_sha256: sha256(source),
            sprints,
            stories,
            items,
        })
    }

    /// Selects the first open sub-task whose dependencies are complete.
    ///
    /// Explicit blockers are skipped but remain open. The selected line and whole-source hashes
    /// make later source edits invalidate the selection.
    ///
    /// # Errors
    ///
    /// Returns [`PlanError`] for unknown blockers, unresolved ambiguity, or no ready work.
    pub fn select_next(&self, blockers: &BTreeSet<String>) -> Result<SelectedWork, PlanError> {
        let by_id: BTreeMap<&str, &PlanItem> = self
            .items
            .iter()
            .map(|item| (item.id.as_str(), item))
            .collect();
        if blockers.iter().any(|id| !by_id.contains_key(id.as_str())) {
            return Err(PlanError::InvalidDependency);
        }
        for item in &self.items {
            if item.kind != PlanItemKind::SubTask
                || item.state == CheckState::Checked
                || blockers.contains(&item.id)
            {
                continue;
            }
            let ready = item.dependencies.iter().all(|id| {
                by_id
                    .get(id.as_str())
                    .is_some_and(|dependency| dependency.state == CheckState::Checked)
            });
            if ready {
                if item.title.len() < 8 || item.title.len() > 4096 {
                    return Err(PlanError::NeedsDecomposition);
                }
                return Ok(SelectedWork {
                    item: item.clone(),
                    source_sha256: self.source_sha256.clone(),
                });
            }
        }
        Err(PlanError::NoReadyWork)
    }

    /// Selects one exact open sub-task after verifying all of its declared dependencies.
    ///
    /// This is the safe operator-directed alternative when earlier open items are known external
    /// blockers that cannot be inferred from Markdown checkbox state alone.
    ///
    /// # Errors
    ///
    /// Returns [`PlanError`] when the identifier is absent, ambiguous, checked, not a sub-task,
    /// dependency-blocked, or too broad for one bounded unit.
    pub fn select_exact(&self, item_id: &str) -> Result<SelectedWork, PlanError> {
        let matches = self
            .items
            .iter()
            .filter(|item| item.id == item_id)
            .collect::<Vec<_>>();
        let [item] = matches.as_slice() else {
            return Err(PlanError::NoReadyWork);
        };
        if item.kind != PlanItemKind::SubTask || item.state != CheckState::Open {
            return Err(PlanError::NoReadyWork);
        }
        let by_id = self
            .items
            .iter()
            .map(|candidate| (candidate.id.as_str(), candidate))
            .collect::<BTreeMap<_, _>>();
        if item.dependencies.iter().any(|dependency| {
            !by_id
                .get(dependency.as_str())
                .is_some_and(|candidate| candidate.state == CheckState::Checked)
        }) {
            return Err(PlanError::InvalidDependency);
        }
        if item.title.len() < 8 || item.title.len() > 4096 {
            return Err(PlanError::NeedsDecomposition);
        }
        Ok(SelectedWork {
            item: (*item).clone(),
            source_sha256: self.source_sha256.clone(),
        })
    }
}

/// One exact dependency-ready selection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedWork {
    /// Selected canonical item.
    pub item: PlanItem,
    /// Whole-source hash at selection time.
    pub source_sha256: String,
}

impl SelectedWork {
    /// Confirms that the canonical source has not changed since selection.
    ///
    /// # Errors
    ///
    /// Returns [`PlanError::StaleSource`] on any byte change.
    pub fn revalidate(&self, source: &[u8]) -> Result<(), PlanError> {
        if sha256(source) == self.source_sha256 {
            Ok(())
        } else {
            Err(PlanError::StaleSource)
        }
    }
}

/// Canonical body of one bounded provider work packet.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkPacketBody {
    /// Packet schema version.
    pub version: u16,
    /// Run identity.
    pub run_id: RunId,
    /// Task identity.
    pub task_id: TaskId,
    /// Repository identity.
    pub repository_id: RepositoryId,
    /// Worktree identity.
    pub worktree_id: WorktreeId,
    /// Canonical source anchor.
    pub source_anchor: SourceAnchor,
    /// Whole task-source hash.
    pub source_sha256: String,
    /// Exact base commit.
    pub base_commit: String,
    /// Exact feature branch.
    pub branch: String,
    /// Exact absolute worktree.
    pub worktree: PathBuf,
    /// Dependencies already satisfied.
    pub dependencies: Vec<String>,
    /// Bounded task scope.
    pub scope: String,
    /// Relative paths writable by the implementation task.
    pub owned_paths: Vec<PathBuf>,
    /// Literal deterministic command vectors.
    pub commands: Vec<Vec<String>>,
    /// Stable acceptance-criterion identifiers and text.
    pub acceptance_criteria: BTreeMap<String, String>,
    /// Content-free risk codes.
    pub risks: Vec<String>,
    /// Resource limits.
    pub limits: BTreeMap<String, u64>,
    /// Prohibited actions.
    pub prohibited_actions: Vec<String>,
    /// Expected relative artifact paths.
    pub expected_artifacts: Vec<PathBuf>,
    /// Required stable blocker-code namespace.
    pub blocker_namespace: String,
}

/// Versioned packet plus hash of its canonical JSON body.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkPacket {
    /// Canonical packet body.
    pub body: WorkPacketBody,
    /// SHA-256 of the canonical serialized body.
    pub body_sha256: String,
}

impl WorkPacket {
    /// Validates, canonically serializes, and hashes a work packet.
    ///
    /// # Errors
    ///
    /// Returns [`PlanError::InvalidPacket`] for missing, escaping, contradictory, or excessive
    /// authority.
    pub fn build(body: WorkPacketBody) -> Result<Self, PlanError> {
        validate_packet(&body)?;
        let encoded = serde_json::to_vec(&body).map_err(|_| PlanError::InvalidPacket)?;
        if encoded.len() > MAX_PACKET_BYTES {
            return Err(PlanError::InvalidPacket);
        }
        Ok(Self {
            body,
            body_sha256: sha256(&encoded),
        })
    }

    /// Recomputes the canonical body hash.
    ///
    /// # Errors
    ///
    /// Returns [`PlanError::StaleSource`] after any body mutation.
    pub fn verify(&self) -> Result<(), PlanError> {
        let rebuilt = Self::build(self.body.clone())?;
        if rebuilt.body_sha256 == self.body_sha256 {
            Ok(())
        } else {
            Err(PlanError::StaleSource)
        }
    }
}

/// One proposed bounded child of an oversized packet.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedUnit {
    /// Stable child identifier.
    pub id: String,
    /// Bounded child scope.
    pub scope: String,
    /// Subset of original owned paths.
    pub owned_paths: Vec<PathBuf>,
    /// Original acceptance-criterion identifiers covered by this child.
    pub acceptance_criteria: Vec<String>,
}

/// Validates that decomposition preserves requirements and cannot expand authority silently.
///
/// # Errors
///
/// Returns [`PlanError::InvalidDecomposition`] when child identifiers collide, acceptance
/// criteria are lost, paths escape original ownership, or a material expansion lacks approval.
pub fn validate_decomposition(
    packet: &WorkPacket,
    units: &[DerivedUnit],
    material_scope_change_approved: bool,
) -> Result<(), PlanError> {
    if units.len() < 2 {
        return Err(PlanError::InvalidDecomposition);
    }
    let original_paths: BTreeSet<&Path> = packet
        .body
        .owned_paths
        .iter()
        .map(PathBuf::as_path)
        .collect();
    let original_criteria: BTreeSet<&str> = packet
        .body
        .acceptance_criteria
        .keys()
        .map(String::as_str)
        .collect();
    let mut ids = BTreeSet::new();
    let mut mapped = BTreeSet::new();
    for unit in units {
        if !valid_id(&unit.id) || !ids.insert(unit.id.as_str()) || unit.scope.trim().is_empty() {
            return Err(PlanError::InvalidDecomposition);
        }
        if unit.owned_paths.iter().any(|path| {
            !safe_relative(path)
                || (!original_paths.contains(path.as_path()) && !material_scope_change_approved)
        }) {
            return Err(PlanError::InvalidDecomposition);
        }
        for criterion in &unit.acceptance_criteria {
            if !original_criteria.contains(criterion.as_str()) {
                return Err(PlanError::InvalidDecomposition);
            }
            mapped.insert(criterion.as_str());
        }
    }
    if mapped != original_criteria {
        return Err(PlanError::InvalidDecomposition);
    }
    Ok(())
}

/// Content-free task-plan failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanError {
    /// Source is empty, excessive, malformed, or unsupported.
    InvalidSource,
    /// Identifier occurs more than once.
    DuplicateId,
    /// Item parent is absent.
    MissingParent,
    /// Item hierarchy conflicts with dotted identity.
    MalformedHierarchy,
    /// Sprint goal is missing.
    MissingGoal,
    /// Dependency metadata is duplicated, unknown, or ambiguous.
    InvalidDependency,
    /// Parent and child checkbox states contradict.
    ConflictingState,
    /// No dependency-ready item exists.
    NoReadyWork,
    /// Selected unit is too vague or oversized.
    NeedsDecomposition,
    /// Canonical source or packet changed.
    StaleSource,
    /// Work packet is invalid or excessive.
    InvalidPacket,
    /// Decomposition loses requirements or expands authority.
    InvalidDecomposition,
}

impl fmt::Display for PlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidSource => "codingmage.plan.invalid_source",
            Self::DuplicateId => "codingmage.plan.duplicate_id",
            Self::MissingParent => "codingmage.plan.missing_parent",
            Self::MalformedHierarchy => "codingmage.plan.malformed_hierarchy",
            Self::MissingGoal => "codingmage.plan.missing_goal",
            Self::InvalidDependency => "codingmage.plan.invalid_dependency",
            Self::ConflictingState => "codingmage.plan.conflicting_state",
            Self::NoReadyWork => "codingmage.plan.no_ready_work",
            Self::NeedsDecomposition => "codingmage.plan.needs_decomposition",
            Self::StaleSource => "codingmage.plan.stale_source",
            Self::InvalidPacket => "codingmage.plan.invalid_packet",
            Self::InvalidDecomposition => "codingmage.plan.invalid_decomposition",
        })
    }
}

impl std::error::Error for PlanError {}

fn split_heading(value: &str) -> Result<(String, String), PlanError> {
    let value = value.trim_end_matches(['\r', '\n']);
    let (id, title) = value.split_once(" - ").ok_or(PlanError::InvalidSource)?;
    if !valid_id(id) || title.trim().is_empty() {
        return Err(PlanError::InvalidSource);
    }
    Ok((id.to_owned(), title.trim().to_owned()))
}

fn sprint_heading(line: &str) -> Option<&str> {
    line.strip_prefix("## Sprint ")
        .or_else(|| line.strip_prefix("### [ ] Sprint "))
        .or_else(|| line.strip_prefix("### [x] Sprint "))
        .filter(|value| numbered_heading(value))
}

fn story_heading(line: &str) -> Option<&str> {
    line.strip_prefix("### Story ")
        .or_else(|| line.strip_prefix("#### [ ] Story "))
        .or_else(|| line.strip_prefix("#### [x] Story "))
        .filter(|value| numbered_heading(value))
}

fn numbered_heading(value: &str) -> bool {
    value
        .split_once(" - ")
        .is_some_and(|(id, title)| valid_id(id) && !title.trim().is_empty())
}

fn missing_parent(line: usize, kind: &str) -> PlanError {
    let _ = (line, kind);
    PlanError::MissingParent
}

fn parse_checkbox(line: &str) -> Option<(CheckState, String)> {
    let trimmed = line.trim_start();
    let (state, rest) = if let Some(rest) = trimmed.strip_prefix("- [ ] ") {
        (CheckState::Open, rest)
    } else if let Some(rest) = trimmed.strip_prefix("- [x] ") {
        (CheckState::Checked, rest)
    } else {
        return None;
    };
    let body = rest.strip_prefix("**")?;
    let end = body.find("**")?;
    let emphasized = &body[..end];
    let trailing = body[end + 2..].trim();
    let label = if trailing.is_empty() {
        emphasized.to_owned()
    } else {
        format!("{emphasized} {trailing}")
    };
    Some((state, label))
}

fn split_item(value: &str) -> Result<(String, String), PlanError> {
    let (identity, title) = value
        .split_once(':')
        .or_else(|| value.split_once(" - "))
        .ok_or(PlanError::InvalidSource)?;
    let id = identity
        .split_ascii_whitespace()
        .next()
        .ok_or(PlanError::InvalidSource)?;
    if !valid_id(id) || title.trim().is_empty() {
        return Err(PlanError::InvalidSource);
    }
    Ok((id.to_owned(), title.trim().to_owned()))
}

fn dotted_parent(value: &str) -> Option<&str> {
    value.rsplit_once('.').map(|(parent, _)| parent)
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.split('.').all(|segment| {
            !segment.is_empty()
                && (segment.bytes().all(|byte| byte.is_ascii_digit())
                    || segment.strip_prefix("AC").is_some_and(|suffix| {
                        !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
                    }))
        })
}

fn insert_id(ids: &mut BTreeSet<String>, namespace: &str, id: &str) -> Result<(), PlanError> {
    if ids.insert(format!("{namespace}:{id}")) {
        Ok(())
    } else {
        Err(PlanError::DuplicateId)
    }
}

fn validate_dependencies(
    items: &[PlanItem],
    id_counts: &BTreeMap<String, usize>,
) -> Result<(), PlanError> {
    for item in items {
        if item.dependencies.iter().any(|dependency| {
            dependency == &item.id || id_counts.get(dependency.as_str()) != Some(&1)
        }) {
            return Err(PlanError::InvalidDependency);
        }
    }
    Ok(())
}

fn validate_states(items: &[PlanItem]) -> Result<(), PlanError> {
    for parent in items {
        if parent.kind == PlanItemKind::Task
            && parent.state == CheckState::Checked
            && items.iter().any(|child| {
                child.kind == PlanItemKind::SubTask
                    && child.parent_id == parent.id
                    && child.state == CheckState::Open
            })
        {
            return Err(PlanError::ConflictingState);
        }
    }
    Ok(())
}

fn validate_packet(body: &WorkPacketBody) -> Result<(), PlanError> {
    if body.version != 1
        || body.scope.trim().is_empty()
        || body.source_sha256.len() != 64
        || !valid_commit(&body.base_commit)
        || body.branch.trim().is_empty()
        || !body.worktree.is_absolute()
        || body.owned_paths.is_empty()
        || body.owned_paths.iter().any(|path| !safe_relative(path))
        || body.commands.is_empty()
        || body.commands.iter().any(|command| {
            command.is_empty()
                || command
                    .iter()
                    .any(|part| part.is_empty() || part.contains('\0'))
        })
        || body.acceptance_criteria.is_empty()
        || body.prohibited_actions.is_empty()
        || body
            .expected_artifacts
            .iter()
            .any(|path| !safe_relative(path))
        || body.blocker_namespace.trim().is_empty()
    {
        return Err(PlanError::InvalidPacket);
    }
    Ok(())
}

fn safe_relative(path: &Path) -> bool {
    !path.is_absolute()
        && path
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
}

fn valid_commit(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn anchor(line: usize, exact: &str) -> SourceAnchor {
    SourceAnchor {
        line,
        line_sha256: sha256(exact.as_bytes()),
    }
}

fn sha256(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAN: &str = "# Plan\n\n## Sprint 0 - Foundation\n\n**Sprint goal:** Build safely.\n\n### Story 0.1 - Parser\n\n- [ ] **Task 0.1.1 - Parse work**\n  - [x] **Sub-task 0.1.1.1:** Parse prior work.\n  - [ ] **Sub-task 0.1.1.2:** Select next work.\n<!-- depends-on: 0.1.1.1 -->\n\n- [ ] **AC 0.1.1:** Selection is exact.\n\n### Sprint 0 Gate\n\n- [ ] **Gate 0.1:** Parser passes.\n";

    const CHECKBOX_HEADING_PLAN: &str = "# Tasks\n\n### [ ] Sprint 42 - Repository Safety\n\n**Sprint goal:** Preserve repository state.\n\n#### [ ] Story 42.1 - Hostile Repositories\n\n- [ ] **Task 42.1.3 - Verify the story**\n  - [ ] **Sub-task 42.1.3.2** (legacy `S-035-ST01`): Exercise hostile repository state.\n\n- [ ] **Story AC 42.1.AC1:** Given hostile state, when inspected, then no command executes.\n- [ ] **Sprint AC 42.AC1:** Every unrelated ref remains exact.\n\n### [ ] Sprint Completion Record Template\n";

    #[test]
    fn parses_checkbox_headings_legacy_suffixes_and_acceptance_ids() {
        let plan = TaskPlan::parse(CHECKBOX_HEADING_PLAN.as_bytes()).unwrap();
        assert_eq!(plan.sprints.len(), 1);
        assert_eq!(plan.stories.len(), 1);
        assert_eq!(plan.select_exact("42.1.3.2").unwrap().item.id, "42.1.3.2");
        assert!(plan.items.iter().any(|item| item.id == "42.1.AC1"));
        assert!(plan.items.iter().any(|item| item.id == "42.AC1"));
    }

    fn body() -> WorkPacketBody {
        WorkPacketBody {
            version: 1,
            run_id: RunId::new("run-1").unwrap(),
            task_id: TaskId::new("task-1").unwrap(),
            repository_id: RepositoryId::new("repo-1").unwrap(),
            worktree_id: WorktreeId::new("worktree-1").unwrap(),
            source_anchor: SourceAnchor {
                line: 10,
                line_sha256: "a".repeat(64),
            },
            source_sha256: "b".repeat(64),
            base_commit: "c".repeat(40),
            branch: "codingmage/task-1".to_owned(),
            worktree: PathBuf::from("/tmp/worktree"),
            dependencies: vec!["0.1.1.1".to_owned()],
            scope: "Implement exact parser behavior.".to_owned(),
            owned_paths: vec![PathBuf::from("src/lib.rs")],
            commands: vec![vec!["cargo".to_owned(), "test".to_owned()]],
            acceptance_criteria: BTreeMap::from([(
                "AC-1".to_owned(),
                "Malformed input fails closed.".to_owned(),
            )]),
            risks: vec!["parser".to_owned()],
            limits: BTreeMap::from([("deadline_ms".to_owned(), 60_000)]),
            prohibited_actions: vec!["Do not publish.".to_owned()],
            expected_artifacts: vec![PathBuf::from("target/test-result.json")],
            blocker_namespace: "codingmage.blocker".to_owned(),
        }
    }

    #[test]
    fn parses_hierarchy_hashes_and_dependency_ready_selection() {
        let plan = TaskPlan::parse(PLAN.as_bytes()).unwrap();
        assert_eq!(plan.sprints[0].goal, "Build safely.");
        assert_eq!(plan.items.len(), 5);
        let selected = plan.select_next(&BTreeSet::new()).unwrap();
        assert_eq!(selected.item.id, "0.1.1.2");
        assert_eq!(selected.revalidate(PLAN.as_bytes()), Ok(()));
        assert_eq!(
            selected.revalidate(format!("{PLAN}\n").as_bytes()),
            Err(PlanError::StaleSource)
        );
    }

    #[test]
    fn blockers_are_skipped_without_becoming_complete() {
        let plan = TaskPlan::parse(PLAN.as_bytes()).unwrap();
        let blockers = BTreeSet::from(["0.1.1.2".to_owned()]);
        assert_eq!(plan.select_next(&blockers), Err(PlanError::NoReadyWork));
        assert_eq!(
            plan.items
                .iter()
                .find(|item| item.id == "0.1.1.2")
                .unwrap()
                .state,
            CheckState::Open
        );
    }

    #[test]
    fn exact_selection_requires_an_open_dependency_ready_subtask() {
        let plan = TaskPlan::parse(PLAN.as_bytes()).unwrap();
        assert_eq!(plan.select_exact("0.1.1.2").unwrap().item.id, "0.1.1.2");
        assert_eq!(plan.select_exact("0.1.1.1"), Err(PlanError::NoReadyWork));
        assert_eq!(plan.select_exact("0.1.1"), Err(PlanError::NoReadyWork));
        assert_eq!(plan.select_exact("9.9.9.9"), Err(PlanError::NoReadyWork));
    }

    #[test]
    fn malformed_hierarchy_duplicates_dependencies_and_states_fail() {
        let wrong_parent = PLAN.replace("Sub-task 0.1.1.2", "Sub-task 0.2.1.2");
        assert_eq!(
            TaskPlan::parse(wrong_parent.as_bytes()),
            Err(PlanError::MalformedHierarchy)
        );
        let duplicate = PLAN.replace("Sub-task 0.1.1.2", "Sub-task 0.1.1.1");
        assert_eq!(
            TaskPlan::parse(duplicate.as_bytes()),
            Err(PlanError::DuplicateId)
        );
        let unknown = PLAN.replace("0.1.1.1 -->", "9.9.9.9 -->");
        assert_eq!(
            TaskPlan::parse(unknown.as_bytes()),
            Err(PlanError::InvalidDependency)
        );
        let conflict = PLAN.replace("[ ] **Task 0.1.1", "[x] **Task 0.1.1");
        assert_eq!(
            TaskPlan::parse(conflict.as_bytes()),
            Err(PlanError::ConflictingState)
        );
    }

    #[test]
    fn checked_acceptance_criterion_does_not_own_story_tasks() {
        let source = "# Plan\n\n## Sprint 0 - Foundation\n\n**Sprint goal:** Build safely.\n\n### Story 0.1 - Parser\n\n- [ ] **Task 0.1.1 - Parse work**\n  - [ ] **Sub-task 0.1.1.1:** Select work.\n\n- [x] **AC 0.1:** Parsing is exact.\n\n### Sprint 0 Gate\n\n- [ ] **Gate 0.1:** Parser passes.\n";
        let plan = TaskPlan::parse(source.as_bytes()).unwrap();
        assert_eq!(
            plan.select_next(&BTreeSet::new()).unwrap().item.id,
            "0.1.1.1"
        );
    }

    #[test]
    fn packet_hash_is_canonical_and_mutation_is_detected() {
        let packet = WorkPacket::build(body()).unwrap();
        assert_eq!(packet.verify(), Ok(()));
        assert_eq!(
            packet.body_sha256,
            WorkPacket::build(body()).unwrap().body_sha256
        );
        let mut changed = packet;
        changed.body.scope.push_str(" changed");
        assert_eq!(changed.verify(), Err(PlanError::StaleSource));
    }

    #[test]
    fn decomposition_preserves_criteria_and_authority() {
        let packet = WorkPacket::build(body()).unwrap();
        let units = vec![
            DerivedUnit {
                id: "1".to_owned(),
                scope: "Implement".to_owned(),
                owned_paths: vec![PathBuf::from("src/lib.rs")],
                acceptance_criteria: vec!["AC-1".to_owned()],
            },
            DerivedUnit {
                id: "2".to_owned(),
                scope: "Verify".to_owned(),
                owned_paths: vec![],
                acceptance_criteria: vec!["AC-1".to_owned()],
            },
        ];
        assert_eq!(validate_decomposition(&packet, &units, false), Ok(()));
        let mut escaping = units;
        escaping[1].owned_paths = vec![PathBuf::from("../outside")];
        assert_eq!(
            validate_decomposition(&packet, &escaping, true),
            Err(PlanError::InvalidDecomposition)
        );
    }

    #[test]
    fn canonical_repository_plan_parses_and_selects_the_first_open_unit() {
        let source = include_bytes!("../../../TASKS.md");
        let plan = TaskPlan::parse(source).unwrap();
        let selected = plan.select_next(&BTreeSet::new()).unwrap();
        assert_eq!(selected.item.id, "16.2.1.2");
        assert_eq!(selected.revalidate(source), Ok(()));
    }
}
