# CodingMage Development Plan

This file is the canonical implementation sequence for CodingMage. It is intentionally granular so a human or coding agent can select one bounded, dependency-ready unit without reconstructing the project from conversation history.

CodingMage is under active implementation. A checked item means its complete implementation, tests, acceptance criteria, and required evidence genuinely exist in this repository. Documentation existence alone does not close an implementation item.

## Execution Rules

1. Work in sprint and dependency order unless an item explicitly declares itself dependency-independent.
2. Select one bounded sub-task or tightly coupled group per implementation cycle.
3. Inspect Git state before every change and publication action.
4. Never modify a target repository outside a CodingMage-owned worktree.
5. Never let two agents write the same worktree concurrently.
6. Never mark an item complete from model prose alone.
7. Run the required deterministic checks before senior model review.
8. Record exact commits, commands, outcomes, limitations, and blockers.
9. Preserve unsupported platform, provider, credential, hardware, and independent-review claims as open.
10. Do not merge, release, incur costs, alter external infrastructure, or weaken policy without explicit product-owner authority.
11. Stop a correction loop after its configured bound and record a dispute instead of looping forever.
12. Keep runtime logs, credentials, target source copies, and local state out of this source repository.

## Universal Definition of Done

Every completed implementation sub-task must satisfy all applicable conditions:

- [ ] **DoD.1:** The implementation is present in the intended ownership boundary and contains no unrelated refactor.
- [ ] **DoD.2:** Focused positive, negative, boundary, and recovery tests pass.
- [ ] **DoD.3:** Formatting, strict linting, and `git diff --check` pass.
- [ ] **DoD.4:** Public contracts and schemas reject unknown fields and invalid state transitions.
- [ ] **DoD.5:** Sensitive values, target source, credentials, and hidden model reasoning are absent from retained evidence.
- [ ] **DoD.6:** Documentation states current behavior and limitations without claiming unexecuted evidence.
- [ ] **DoD.7:** The source commit and deterministic evidence are linked by immutable identity.
- [ ] **DoD.8:** The working tree is clean and the intended branch is synchronized after publication.

---

## Sprint 0 - Repository, Scope, and Governance Baseline

**Sprint goal:** Establish an unambiguous, reviewable project boundary before executable orchestration begins.

### Story 0.1 - Canonical Product Boundary

**User-facing value:** As a maintainer, I need one authoritative description of CodingMage so implementation cannot silently become a general-purpose autonomous shell or alternate release authority.

- [x] **Task 0.1.1 - Create the initial project explanation**
  - [x] **Sub-task 0.1.1.1:** Define CodingMage as a local development coordinator rather than a coding model or unrestricted agent.
  - [x] **Sub-task 0.1.1.2:** Define the initial Claude Code implementer and Codex senior-review roles.
  - [x] **Sub-task 0.1.1.3:** Define deterministic local tools as the authority for mechanical verification.
  - [x] **Sub-task 0.1.1.4:** State that the repository is planning-only and proposed commands are not implemented.
- [x] **Task 0.1.2 - Define initial authority boundaries**
  - [x] **Sub-task 0.1.2.1:** Enumerate initially permitted local operations.
  - [x] **Sub-task 0.1.2.2:** Enumerate prohibited destructive, external, credential, merge, release, and cost-bearing operations.
  - [x] **Sub-task 0.1.2.3:** Define human product-owner authority and bounded agent authority.

**Story acceptance criteria**

- [x] **AC 0.1.1:** Given the README, when a reviewer inspects the roles and authority sections, then no agent has merge, release, destructive Git, credential, or external-infrastructure authority.
- [x] **AC 0.1.2:** Given proposed commands and features, when current status is reviewed, then every unimplemented capability is visibly identified as planned.

### Story 0.2 - Planning and Contribution Controls

**User-facing value:** As a contributor, I need stable planning, security, and decision rules so future automation can distinguish requirements from suggestions.

- [x] **Task 0.2.1 - Add foundational policy documents**
  - [x] **Sub-task 0.2.1.1:** Choose and add a public software license after explicit owner approval.
  - [x] **Sub-task 0.2.1.2:** Add `SECURITY.md` with private vulnerability reporting, supported versions, response expectations, and safe-disable behavior.
  - [x] **Sub-task 0.2.1.3:** Add `CONTRIBUTING.md` with branch, test, review, evidence, and commit expectations.
  - [x] **Sub-task 0.2.1.4:** Add `CODE_OF_CONDUCT.md` if the repository will accept external contributions. External contributions are closed during bootstrap, so the conditional document is deferred.
- [x] **Task 0.2.2 - Add architectural decision records**
  - [x] **Sub-task 0.2.2.1:** Create an ADR template with context, decision, alternatives, consequences, status, and supersession fields.
  - [x] **Sub-task 0.2.2.2:** Record the Rust-first bootstrap coordinator decision.
  - [x] **Sub-task 0.2.2.3:** Record the external-to-target and no-self-modification decision.
  - [x] **Sub-task 0.2.2.4:** Record the CLI-adapter-first provider boundary.
  - [x] **Sub-task 0.2.2.5:** Record the append-only journal plus atomic snapshot decision.
- [x] **Task 0.2.3 - Establish documentation checks**
  - [x] **Sub-task 0.2.3.1:** Add Markdown formatting and lint configuration.
  - [x] **Sub-task 0.2.3.2:** Add local-link validation.
  - [x] **Sub-task 0.2.3.3:** Add Mermaid syntax validation.
  - [x] **Sub-task 0.2.3.4:** Add prohibited-claim checks for unimplemented, merged, released, approved, or independently reviewed behavior.
  - [x] **Sub-task 0.2.3.5:** Add secret-pattern checks that report locations without printing matched values.

**Story acceptance criteria**

- [x] **AC 0.2.1:** Given a clean checkout, when the documentation gate runs, then Markdown, links, Mermaid, claims, and synthetic-secret fixtures produce deterministic pass or fail results.
- [x] **AC 0.2.2:** Given a material architectural change, when it is proposed, then an ADR records its authority, consequences, and relationship to existing decisions before implementation.

### Sprint 0 Gate

- [x] **Gate 0.1:** README, TASKS, security policy, contribution policy, license, and accepted ADRs agree on scope and authority.
- [x] **Gate 0.2:** Documentation and secret checks pass from a clean checkout. See `docs/evidence/sprint-0.md`.
- [x] **Gate 0.3:** No executable coordinator behavior is claimed before code and tests exist.

---

## Sprint 1 - Rust Workspace and Core Contracts

**Sprint goal:** Create the smallest compilable Rust workspace with typed identities and fail-closed contract validation.

### Story 1.1 - Workspace Bootstrap

- [x] **Task 1.1.1 - Create the workspace**
  - [x] **Sub-task 1.1.1.1:** Add a pinned Rust toolchain and supported minimum Rust version.
  - [x] **Sub-task 1.1.1.2:** Create `codingmage-contracts`, `codingmage-core`, and `codingmage-cli` crates.
  - [x] **Sub-task 1.1.1.3:** Deny warnings in continuous integration and define strict Clippy settings.
  - [x] **Sub-task 1.1.1.4:** Add formatting, unit-test, and documentation commands.
  - [x] **Sub-task 1.1.1.5:** Add `.gitignore` entries for build output, local state, logs, worktrees, credentials, and provider caches.
- [x] **Task 1.1.2 - Define dependency direction**
  - [x] **Sub-task 1.1.2.1:** Prevent contracts from depending on runtime, Git, provider, monitor, or CLI crates.
  - [x] **Sub-task 1.1.2.2:** Prevent provider adapters from bypassing core orchestration decisions.
  - [x] **Sub-task 1.1.2.3:** Add an automated dependency-direction test.

**Story acceptance criteria**

- [x] **AC 1.1.1:** Given a clean checkout, when the workspace build and tests run, then all crates compile with pinned dependencies and zero warnings.
- [x] **AC 1.1.2:** Given a seeded forbidden dependency, when the architecture test runs, then it names and rejects the exact edge.

### Story 1.2 - Stable Identities and Errors

- [x] **Task 1.2.1 - Define identifier types**
  - [x] **Sub-task 1.2.1.1:** Add validated `RepositoryId`, `RunId`, `TaskId`, `WorktreeId`, `AgentId`, `AttemptId`, `ReviewId`, and `EvidenceId` types.
  - [x] **Sub-task 1.2.1.2:** Reject empty, oversized, path-bearing, control-character, and ambiguous identifiers.
  - [x] **Sub-task 1.2.1.3:** Add canonical serialization and ordering tests.
- [x] **Task 1.2.2 - Define content-free errors**
  - [x] **Sub-task 1.2.2.1:** Create stable error codes for configuration, repository, Git, process, provider, gate, state, quota, and evidence failures.
  - [x] **Sub-task 1.2.2.2:** Ensure errors carry bounded metadata without source excerpts, commands containing secrets, or environment dumps.
  - [x] **Sub-task 1.2.2.3:** Add round-trip and unknown-code compatibility tests.

**Story acceptance criteria**

- [x] **AC 1.2.1:** Given malformed and adversarial identifier inputs, when constructors or deserializers run, then every invalid value fails before becoming authority.
- [x] **AC 1.2.2:** Given synthetic secrets in lower-level failures, when public errors serialize, then no secret value appears.

### Sprint 1 Gate

- [x] **Gate 1.1:** Workspace, dependency direction, identifiers, and errors pass focused tests and strict Clippy. See `docs/evidence/sprint-1.md`.
- [x] **Gate 1.2:** Generated API documentation contains no unimplemented capability claim.

---

## Sprint 2 - Configuration and Target Authorization

**Sprint goal:** Admit only explicit target repositories and typed, versioned configuration.

### Story 2.1 - Configuration Contract

- [x] **Task 2.1.1 - Define project configuration**
  - [x] **Sub-task 2.1.1.1:** Define a versioned `codingmage.toml` schema.
  - [x] **Sub-task 2.1.1.2:** Include target path, task source, default branch, integration branch, scratch root, agent profiles, correction limits, gate commands, and publication policy.
  - [x] **Sub-task 2.1.1.3:** Require explicit opt-in for network, push, issue, and pull-request capabilities.
  - [x] **Sub-task 2.1.1.4:** Reject unknown fields, duplicate keys, unsupported versions, relative authority roots, and conflicting policies.
- [x] **Task 2.1.2 - Add configuration loading**
  - [x] **Sub-task 2.1.2.1:** Load only the explicitly selected configuration file.
  - [x] **Sub-task 2.1.2.2:** Prohibit ambient parent-directory discovery unless explicitly enabled.
  - [x] **Sub-task 2.1.2.3:** Canonicalize paths without following an unapproved repository replacement.
  - [x] **Sub-task 2.1.2.4:** Produce a redacted effective-configuration view.

**Story acceptance criteria**

- [x] **AC 2.1.1:** Given valid configuration, when loaded twice, then canonical output is byte-identical.
- [x] **AC 2.1.2:** Given unknown, conflicting, traversing, or secret-bearing configuration, when loading runs, then it fails closed without printing sensitive values.

### Story 2.2 - Repository Authorization

- [x] **Task 2.2.1 - Bind repository identity**
  - [x] **Sub-task 2.2.1.1:** Resolve the target through a held directory handle where supported.
  - [x] **Sub-task 2.2.1.2:** Record canonical path, filesystem identity, Git directory identity, initial `HEAD`, and remote identities.
  - [x] **Sub-task 2.2.1.3:** Reject bare repositories, nested ambiguity, symlink replacement, unsafe ownership, and unsupported repository formats by default.
  - [x] **Sub-task 2.2.1.4:** Revalidate repository identity before every state-changing phase.
- [x] **Task 2.2.2 - Enforce self-target prohibition**
  - [x] **Sub-task 2.2.2.1:** Detect when CodingMage is pointed at its own source or runtime-state directory.
  - [x] **Sub-task 2.2.2.2:** Reject overlapping source, scratch, state, and target roots.
  - [x] **Sub-task 2.2.2.3:** Add symlink, bind-path, and renamed-directory fixtures. The bind-path predicate is verified with synthetic equal physical identities without requiring a privileged mount.

**Story acceptance criteria**

- [x] **AC 2.2.1:** Given a replaced or moved target after authorization, when revalidation runs, then no command or write executes.
- [x] **AC 2.2.2:** Given CodingMage itself or an overlapping state root as target, when authorization runs, then the request is denied with a stable code.

### Sprint 2 Gate

- [x] **Gate 2.1:** Configuration and repository identity mutation suites pass. See `docs/evidence/sprint-2.md`.
- [x] **Gate 2.2:** No target operation can begin from implicit or stale authorization.

---

## Sprint 3 - Git Safety and Worktree Isolation

**Sprint goal:** Create and manage exact CodingMage-owned Git worktrees without disturbing user-owned repository state.

### Story 3.1 - Read-Only Repository Inventory

- [x] **Task 3.1.1 - Capture repository state**
  - [x] **Sub-task 3.1.1.1:** Record `HEAD`, current branch/detached state, index identity, porcelain status, refs, tags, notes, stash, worktrees, configuration, hooks, in-progress operations, and remotes.
  - [x] **Sub-task 3.1.1.2:** Bound output, records, runtime, and retained data.
  - [x] **Sub-task 3.1.1.3:** Disable aliases, pagers, editors, signers, hooks, filters, helpers, replacement refs, alternates, and ambient Git variables.
  - [x] **Sub-task 3.1.1.4:** Prohibit network during local inventory.
- [x] **Task 3.1.2 - Detect dirty and unsafe state**
  - [x] **Sub-task 3.1.2.1:** Classify clean, staged, unstaged, untracked, detached, conflicted, rebasing, merging, bisecting, and locked states.
  - [x] **Sub-task 3.1.2.2:** Refuse unknown or unsupported states without altering them.
  - [x] **Sub-task 3.1.2.3:** Preserve unrelated user changes and notes exactly.

**Story acceptance criteria**

- [x] **AC 3.1.1:** Given hostile Git configuration, when inventory runs, then no executable canary fires and no network connection occurs.
- [x] **AC 3.1.2:** Given every supported dirty state, when inventory completes, then the state is classified without mutation.

### Story 3.2 - Owned Worktrees and Branches

- [x] **Task 3.2.1 - Create owned worktrees**
  - [x] **Sub-task 3.2.1.1:** Generate collision-resistant branch and worktree identities.
  - [x] **Sub-task 3.2.1.2:** Create under the configured scratch root from an exact source commit.
  - [x] **Sub-task 3.2.1.3:** Record ownership manifest, expected path identity, source commit, task, run, and process owner.
  - [x] **Sub-task 3.2.1.4:** Apply private permissions and reject preexisting destinations.
- [x] **Task 3.2.2 - Revalidate and remove owned worktrees**
  - [x] **Sub-task 3.2.2.1:** Revalidate path, Git registration, branch, source lineage, and ownership manifest before mutation.
  - [x] **Sub-task 3.2.2.2:** Fail closed if the worktree is missing, renamed, replaced, or no longer owned.
  - [x] **Sub-task 3.2.2.3:** Remove only the exact owned worktree and retain a truthful result.
  - [x] **Sub-task 3.2.2.4:** Preserve active checkout, index, files, refs, notes, stash, tags, configuration, and hooks.
- [x] **Task 3.2.3 - Enforce prohibited Git operations**
  - [x] **Sub-task 3.2.3.1:** Deny force push, reset, clean, checkout-overwrite, branch deletion, prune, garbage collection, and history rewriting.
  - [x] **Sub-task 3.2.3.2:** Deny default-branch writes and merges in the bootstrap release.
  - [x] **Sub-task 3.2.3.3:** Add literal argument templates rather than shell-composed Git commands.

**Story acceptance criteria**

- [x] **AC 3.2.1:** Given clean, dirty, detached, missing, renamed, and concurrently changed repositories, when worktree lifecycle operations run, then only exact CodingMage-owned artifacts change.
- [x] **AC 3.2.2:** Given every prohibited Git operation, when requested through configuration, model output, or task content, then it is rejected before process spawn.

### Sprint 3 Gate

- [x] **Gate 3.1:** Worktree lifecycle and hostile-repository fixtures pass repeatedly with zero user-state drift. See `docs/evidence/sprint-3.md`.
- [x] **Gate 3.2:** Crash cleanup identifies owned artifacts exactly and never by name-pattern adoption. Recovery loads one validated manifest identifier and leaves similarly named directories untouched.

---

## Sprint 4 - Bounded Process Runtime and Agent Contract

**Sprint goal:** Execute provider CLIs through one constrained, observable process boundary.

### Story 4.1 - Process Execution Contract

- [x] **Task 4.1.1 - Define process request and result schemas**
  - [x] **Sub-task 4.1.1.1:** Define executable identity, literal arguments, working directory, environment template, stdin source, output bounds, deadline, cancellation, and expected exit classifications.
  - [x] **Sub-task 4.1.1.2:** Define stdout/stderr digest, truncation, elapsed time, process identity, descendant cleanup, and terminal outcome fields.
  - [x] **Sub-task 4.1.1.3:** Reject shell strings, unknown flags, unbounded output, absent deadlines, and ambient environment inheritance.
- [x] **Task 4.1.2 - Implement bounded subprocesses**
  - [x] **Sub-task 4.1.2.1:** Resolve and pin executable identity before spawn.
  - [x] **Sub-task 4.1.2.2:** Clear environment and add only declared variables.
  - [x] **Sub-task 4.1.2.3:** Use literal argument vectors and explicit working directories.
  - [x] **Sub-task 4.1.2.4:** Bound output, runtime, retries, processes, and open descriptors. The runtime performs exactly one attempt; orchestration-level retries are not process authority.
  - [x] **Sub-task 4.1.2.5:** Kill and reap the exact process tree on timeout, cancellation, or parent failure.

**Story acceptance criteria**

- [x] **AC 4.1.1:** Given metacharacters, response files, configuration discovery, environment injection, and executable replacement, when execution is attempted, then no alternate interpretation occurs.
- [x] **AC 4.1.2:** Given cancellation, timeout, crash, and descendant spawning fixtures, when the process ends, then every owned descendant is terminated and exactly one truthful terminal result exists.

### Story 4.2 - Provider-Neutral Agent Adapter

- [x] **Task 4.2.1 - Define adapter operations**
  - [x] **Sub-task 4.2.1.1:** Define capability probe, start, continue, cancel, usage observation, and result normalization operations.
  - [x] **Sub-task 4.2.1.2:** Define implementation, review, correction, verification, and administrative roles.
  - [x] **Sub-task 4.2.1.3:** Define structured event streaming and final-response schemas.
  - [x] **Sub-task 4.2.1.4:** Treat all provider output as untrusted data.
- [x] **Task 4.2.2 - Build fake adapters**
  - [x] **Sub-task 4.2.2.1:** Add deterministic success, failure, malformed output, timeout, quota, cancellation, and contradictory-result fixtures.
  - [x] **Sub-task 4.2.2.2:** Add scripted multi-turn implementation and review conversations.
  - [x] **Sub-task 4.2.2.3:** Prove adapters cannot directly advance task state or perform Git publication.

**Story acceptance criteria**

- [x] **AC 4.2.1:** Given any provider event order, when normalization runs, then only schema-valid ordered events reach the coordinator.
- [x] **AC 4.2.2:** Given a malicious adapter result claiming tests passed or a merge completed, when received, then no state advances without independent evidence.

### Sprint 4 Gate

- [x] **Gate 4.1:** Process containment, output bounds, cancellation, and fake-adapter campaigns pass. See `docs/evidence/sprint-4.md`.
- [x] **Gate 4.2:** Provider adapters have no direct repository, task-state, merge, or release authority.

---

## Sprint 5 - Claude Code Implementation Adapter

**Sprint goal:** Run Claude Code as a bounded implementation agent with structured input and output.

### Story 5.1 - Claude Capability and Session Management

- [x] **Task 5.1.1 - Probe installed Claude Code**
  - [x] **Sub-task 5.1.1.1:** Verify executable identity and supported version range.
  - [x] **Sub-task 5.1.1.2:** Probe non-interactive mode, JSON output, JSON Schema output, session resume, model selection, effort selection, and permission modes.
  - [x] **Sub-task 5.1.1.3:** Record supported capabilities without reading or copying authentication material.
  - [x] **Sub-task 5.1.1.4:** Fail visibly when required flags or behavior are unavailable.
- [x] **Task 5.1.2 - Implement session lifecycle**
  - [x] **Sub-task 5.1.2.1:** Start a named implementation session bound to one run, task, repository, and worktree.
  - [x] **Sub-task 5.1.2.2:** Continue only the exact retained session.
  - [x] **Sub-task 5.1.2.3:** Detect context exhaustion, provider errors, rate limits, and session disappearance.
  - [x] **Sub-task 5.1.2.4:** Cancel and reap the exact Claude process tree.

### Story 5.2 - Claude Work Packet and Permissions

- [x] **Task 5.2.1 - Render implementation instructions**
  - [x] **Sub-task 5.2.1.1:** Include exact task text, dependencies, owned paths, source commit, branch, acceptance criteria, test commands, and prohibited actions.
  - [x] **Sub-task 5.2.1.2:** Mark repository text, issues, comments, fixtures, and tool output as untrusted content rather than instructions.
  - [x] **Sub-task 5.2.1.3:** Require a structured completion report with changes, test claims, candidate readiness or commit disposition, limitations, and blockers.
- [x] **Task 5.2.2 - Enforce implementation authority.** Local authority construction and coordinator-owned commit fixtures pass; authenticated live-provider evidence remains under the acceptance gates. See `docs/evidence/sprint-5-local.md`.
  - [x] **Sub-task 5.2.2.1:** Allow file reads and writes only in the assigned worktree, excluding its Git metadata, through a bare deny-first provider profile.
  - [x] **Sub-task 5.2.2.2:** Expose only scoped file tools to Claude; deterministic local commands and literal Git operations remain coordinator-owned.
  - [x] **Sub-task 5.2.2.3:** Deny Bash, web, subagent, skill, notebook, merge, release, default-branch push, destructive Git, credential, and external-infrastructure authority.
  - [x] **Sub-task 5.2.2.4:** Refuse a completion report that lacks exactly one coherent commit, ready-for-coordinator-commit disposition, or truthful blocker.

**Story acceptance criteria**

- [ ] **AC 5.1:** Given fake and live disposable repositories, when Claude performs a bounded task, then changes remain inside its worktree and its structured report binds to the actual commit.
- [ ] **AC 5.2:** Given malicious repository instructions or a request for prohibited authority, when Claude runs, then CodingMage blocks the effect regardless of model response.

### Sprint 5 Gate

- [ ] **Gate 5.1:** Claude adapter fixtures and one disposable live task pass with exact session, commit, process, and permission evidence.
- [ ] **Gate 5.2:** No Claude authentication secret or hidden reasoning appears in CodingMage state or logs.

---

## Sprint 6 - Codex Senior Review Adapter

**Sprint goal:** Run Codex against exact commits as a read-only senior reviewer and verifier.

### Story 6.1 - Codex Capability and Thread Management

- [x] **Task 6.1.1 - Probe installed Codex**
  - [x] **Sub-task 6.1.1.1:** Verify executable identity and supported version range.
  - [x] **Sub-task 6.1.1.2:** Probe non-interactive execution, JSONL events, output schemas, thread continuation, model selection, effort selection, and sandbox modes.
  - [x] **Sub-task 6.1.1.3:** Record supported capabilities without reading or copying authentication material.
  - [x] **Sub-task 6.1.1.4:** Fail visibly when required review behavior is unavailable.
- [x] **Task 6.1.2 - Implement review thread lifecycle**
  - [x] **Sub-task 6.1.2.1:** Start a review thread bound to one target commit, base commit, task, and evidence set.
  - [x] **Sub-task 6.1.2.2:** Resume only the exact thread for correction verification.
  - [x] **Sub-task 6.1.2.3:** Run review with read-only repository authority.
  - [x] **Sub-task 6.1.2.4:** Cancel and reap the exact Codex process tree.

### Story 6.2 - Structured Senior Findings

- [x] **Task 6.2.1 - Define review output**
  - [x] **Sub-task 6.2.1.1:** Require `pass`, `changes_required`, `disputed`, or `blocked` verdicts.
  - [x] **Sub-task 6.2.1.2:** Require stable finding ID, severity, file, line, claim, evidence, requested correction, and acceptance test. File and line are mandatory for implementation defects and nullable for separately typed external blockers or suggestions.
  - [x] **Sub-task 6.2.1.3:** Reject findings against files or commits outside the bound review scope.
  - [x] **Sub-task 6.2.1.4:** Separate implementation defects from external blockers and optional suggestions.
- [x] **Task 6.2.2 - Verify review integrity**
  - [x] **Sub-task 6.2.2.1:** Revalidate target and base commits before and after review.
  - [x] **Sub-task 6.2.2.2:** Verify referenced files and lines exist in the reviewed commit.
  - [x] **Sub-task 6.2.2.3:** Prevent Codex prose from marking tests, tasks, or gates complete.

**Story acceptance criteria**

- [x] **AC 6.1:** Given a known-defective commit corpus, when Codex reviews each exact commit, then normalized findings point only to that commit and malformed findings fail closed.
- [x] **AC 6.2:** Given repository changes during review, when commit identity is revalidated, then the review is rejected as stale rather than applied to newer work.

### Sprint 6 Gate

- [x] **Gate 6.1:** Codex review and resume fixtures pass under read-only authority. See `docs/evidence/sprint-6-review-scope.md`.
- [x] **Gate 6.2:** Codex has no write, publication, merge, release, or task-state authority.

---

## Sprint 7 - Task Sources and Work Packets

**Sprint goal:** Convert large plans into exact, dependency-ready units without allowing prose to become executable authority.

### Story 7.1 - Markdown Task Plan Adapter

- [x] **Task 7.1.1 - Parse structured checklist plans**
  - [x] **Sub-task 7.1.1.1:** Parse sprint, story, task, sub-task, goal, dependency, acceptance criteria, and checkbox state.
  - [x] **Sub-task 7.1.1.2:** Preserve source anchors and exact text hashes.
  - [x] **Sub-task 7.1.1.3:** Detect duplicate IDs, malformed nesting, missing parents, conflicting states, and ambiguous dependencies.
  - [x] **Sub-task 7.1.1.4:** Read without rewriting or reformatting the source plan.
- [x] **Task 7.1.2 - Select dependency-ready work**
  - [x] **Sub-task 7.1.2.1:** Select the first open unit whose declared dependencies are complete or explicitly nonblocking.
  - [x] **Sub-task 7.1.2.2:** Skip external blockers without representing them as complete.
  - [x] **Sub-task 7.1.2.3:** Reject vague, oversized, or internally contradictory units for decomposition.
  - [x] **Sub-task 7.1.2.4:** Persist the selected source hash so changed plans invalidate stale work.

### Story 7.2 - Bounded Work Packet

- [x] **Task 7.2.1 - Define work-packet schema**
  - [x] **Sub-task 7.2.1.1:** Include identities, source anchors, dependencies, scope, owned paths, commands, acceptance criteria, risks, limits, and prohibited actions.
  - [x] **Sub-task 7.2.1.2:** Include exact repository, base commit, branch, and worktree bindings.
  - [x] **Sub-task 7.2.1.3:** Include expected artifacts and truthful-blocker format.
  - [x] **Sub-task 7.2.1.4:** Canonically hash and version every packet.
- [x] **Task 7.2.2 - Add decomposition flow**
  - [x] **Sub-task 7.2.2.1:** Allow Codex to propose smaller units without changing canonical scope.
  - [x] **Sub-task 7.2.2.2:** Require deterministic validation and product-owner policy for material scope changes.
  - [x] **Sub-task 7.2.2.3:** Link derived units back to the original task and acceptance criteria.

**Story acceptance criteria**

- [x] **AC 7.1:** Given valid, malformed, contradictory, and changing task plans, when selection runs, then only one exact dependency-ready unit is claimed and stale packets are rejected.
- [x] **AC 7.2:** Given an oversized task, when decomposition runs, then every original requirement remains mapped and no new authority appears.

### Sprint 7 Gate

- [x] **Gate 7.1:** Task parser corpus, dependency selection, plan mutation, and work-packet schemas pass. See `docs/evidence/sprint-7.md`.
- [x] **Gate 7.2:** CodingMage can select and explain the next unit without editing the plan.

---

## Sprint 8 - Deterministic Gate Runner

**Sprint goal:** Reject mechanically invalid work before spending senior-review capacity.

### Story 8.1 - Gate Registry and Execution

- [x] **Task 8.1.1 - Define gate profiles**
  - [x] **Sub-task 8.1.1.1:** Define Tier 0 through Tier 4 gate identities and triggers.
  - [x] **Sub-task 8.1.1.2:** Define literal executable, arguments, working directory, environment, deadline, output, and expected-result contracts.
  - [x] **Sub-task 8.1.1.3:** Define required, optional, unavailable, skipped-with-policy, and failed outcomes.
  - [x] **Sub-task 8.1.1.4:** Prohibit model-generated executable gate definitions.
- [x] **Task 8.1.2 - Implement gate execution**
  - [x] **Sub-task 8.1.2.1:** Run independent gates concurrently only when their declared resources do not conflict.
  - [x] **Sub-task 8.1.2.2:** Stream bounded progress and retain sanitized output digests.
  - [x] **Sub-task 8.1.2.3:** Cancel remaining gates after a configured blocking failure.
  - [x] **Sub-task 8.1.2.4:** Reap every owned process and release locks.

### Story 8.2 - Evidence and Mutation Checks

- [x] **Task 8.2.1 - Produce gate evidence**
  - [x] **Sub-task 8.2.1.1:** Bind gate definition, source commit, executable identity, environment profile, start/end time, outcome, and output digest.
  - [x] **Sub-task 8.2.1.2:** Record truncation and unavailable evidence visibly.
  - [x] **Sub-task 8.2.1.3:** Prevent a successful process exit from substituting for expected assertions.
- [x] **Task 8.2.2 - Test gate integrity**
  - [x] **Sub-task 8.2.2.1:** Mutate command, arguments, timeout, source commit, expected result, and output digest independently.
  - [x] **Sub-task 8.2.2.2:** Assert every mutation invalidates the evidence or gate decision.

**Story acceptance criteria**

- [x] **AC 8.1:** Given passing, failing, hanging, noisy, and malformed gate fixtures, when executed, then outcomes and cleanup are deterministic and bounded.
- [x] **AC 8.2:** Given tampered evidence, when verification runs, then no senior review or task completion may rely on it.

### Sprint 8 Gate

- [x] **Gate 8.1:** Tiered gate scheduling, process cleanup, evidence, and mutation suites pass. See `docs/evidence/sprint-8.md`.
- [x] **Gate 8.2:** A failed required gate prevents model review and advancement.

---

## Sprint 9 - Risk Classification and Model Routing

**Sprint goal:** Select efficient models without silently weakening high-risk engineering review.

### Story 9.1 - Deterministic Risk Classification

- [x] **Task 9.1.1 - Define risk signals**
  - [x] **Sub-task 9.1.1.1:** Classify security, authentication, credentials, cryptography, concurrency, process control, Git mutation, persistence, cross-platform, packaging, release, and architecture paths as elevated risk.
  - [x] **Sub-task 9.1.1.2:** Include task labels, changed paths, dependency breadth, diff shape, prior failures, and unresolved findings.
  - [x] **Sub-task 9.1.1.3:** Ensure unrecognized signals increase rather than reduce review strength.
- [x] **Task 9.1.2 - Produce routing decision**
  - [x] **Sub-task 9.1.2.1:** Define requested provider, role, model profile, effort, speed, reason codes, and escalation conditions.
  - [x] **Sub-task 9.1.2.2:** Record exact resolved model identity when the provider exposes it.
  - [x] **Sub-task 9.1.2.3:** Reject unavailable profiles rather than silently falling back across a required gate.

### Story 9.2 - Adaptive Escalation

- [x] **Task 9.2.1 - Implement initial routing table**
  - [x] **Sub-task 9.2.1.1:** Route routine Claude implementation to Sonnet.
  - [x] **Sub-task 9.2.1.2:** Route high-risk or repeatedly failed implementation to Opus.
  - [x] **Sub-task 9.2.1.3:** Route routine Codex review to Terra High.
  - [x] **Sub-task 9.2.1.4:** Route high-risk, disputed, and final story reviews to Sol High.
  - [x] **Sub-task 9.2.1.5:** Route mechanical administration to deterministic code or Luna without gate authority.
- [x] **Task 9.2.2 - Add performance feedback**
  - [x] **Sub-task 9.2.2.1:** Track elapsed time, retries, correction count, gate failures, review findings, and exposed usage metrics.
  - [x] **Sub-task 9.2.2.2:** Escalate after configured failure or disagreement thresholds.
  - [x] **Sub-task 9.2.2.3:** Never downgrade a mandatory final gate based solely on usage pressure.
  - [x] **Sub-task 9.2.2.4:** Allow an explicit operator pin or override with journaled reason.

**Story acceptance criteria**

- [x] **AC 9.1:** Given a fixed task/diff corpus, when classification runs repeatedly, then routing decisions are identical and high-risk cases never reach a weaker gate.
- [x] **AC 9.2:** Given provider unavailability or quota pressure, when routing cannot satisfy policy, then work pauses rather than silently substituting an unauthorized model.

### Sprint 9 Gate

- [x] **Gate 9.1:** Risk corpus and routing mutation tests pass. See `docs/evidence/sprint-9.md`.
- [x] **Gate 9.2:** Every model decision is explainable from retained non-sensitive inputs.

---

## Sprint 10 - Orchestration State Machine

**Sprint goal:** Execute exactly one legal transition at a time and recover safely from every interruption point.

### Story 10.1 - Run and Task Lifecycle

- [x] **Task 10.1.1 - Define lifecycle states and events**
  - [x] **Sub-task 10.1.1.1:** Implement discovered, ready, claimed, implementing, local-verification, senior-review, correcting, final-verification, checkpointed, and complete states.
  - [x] **Sub-task 10.1.1.2:** Implement blocked, paused, recoverable-failure, terminal-failure, and cancelled states.
  - [x] **Sub-task 10.1.1.3:** Define allowed prior state, triggering evidence, resulting state, and side-effect intent for every transition.
  - [x] **Sub-task 10.1.1.4:** Reject duplicate, stale, reordered, skipped, or contradictory transitions.
- [x] **Task 10.1.2 - Implement one-unit coordinator**
  - [x] **Sub-task 10.1.2.1:** Discover and claim one task.
  - [x] **Sub-task 10.1.2.2:** Create one owned worktree and implementation session.
  - [x] **Sub-task 10.1.2.3:** Execute local gates and senior review.
  - [x] **Sub-task 10.1.2.4:** Checkpoint pass, correction, block, or failure outcomes.
  - [x] **Sub-task 10.1.2.5:** Release locks and owned processes on every terminal path.

### Story 10.2 - Multi-Unit Progression

- [x] **Task 10.2.1 - Advance through dependency-ready work**
  - [x] **Sub-task 10.2.1.1:** Reparse and revalidate the canonical task source after every accepted unit.
  - [x] **Sub-task 10.2.1.2:** Refuse to advance if completion evidence or plan state disagrees.
  - [x] **Sub-task 10.2.1.3:** Continue past precise external blockers when downstream work is independently safe.
  - [x] **Sub-task 10.2.1.4:** Stop when no dependency-ready work remains.

**Story acceptance criteria**

- [x] **AC 10.1:** Given every interruption point in a scripted fake-agent workflow, when recovery runs, then the state machine resumes or stops from the last durable legal transition without duplicate effects.
- [x] **AC 10.2:** Given reordered, replayed, or fabricated events, when applied, then state and repository remain unchanged.

### Sprint 10 Gate

- [x] **Gate 10.1:** Exhaustive state-transition and interruption tests pass. See `docs/evidence/sprint-12.md`.
- [x] **Gate 10.2:** One complete fake-model vertical slice advances exactly one fixture task. See `docs/evidence/sprint-10-local.md`.

---

## Sprint 11 - Review, Correction, and Disagreement Control

**Sprint goal:** Turn senior findings into bounded corrections without endless model argument or self-review.

### Story 11.1 - Finding Lifecycle

- [x] **Task 11.1.1 - Validate and track findings**
  - [x] **Sub-task 11.1.1.1:** Deduplicate findings by stable identity and reviewed commit.
  - [x] **Sub-task 11.1.1.2:** Track open, accepted, corrected, verified, disputed, withdrawn, and blocked states.
  - [x] **Sub-task 11.1.1.3:** Require correction commits to reference addressed finding IDs.
  - [x] **Sub-task 11.1.1.4:** Reject a finding as verified when the relevant code or test did not change and no valid explanation exists.
- [x] **Task 11.1.2 - Build correction packets**
  - [x] **Sub-task 11.1.2.1:** Include only validated findings, exact reviewed/correction commits, requested tests, and unchanged task scope.
  - [x] **Sub-task 11.1.2.2:** Prevent optional suggestions from becoming mandatory scope silently.

### Story 11.2 - Bounded Review Loop

- [x] **Task 11.2.1 - Enforce correction budget**
  - [x] **Sub-task 11.2.1.1:** Set a configurable default maximum of three review/correction rounds.
  - [x] **Sub-task 11.2.1.2:** Escalate implementation model or review model according to policy before the final round.
  - [x] **Sub-task 11.2.1.3:** Record a dispute or blocker when the bound is reached.
  - [x] **Sub-task 11.2.1.4:** Require human resolution for material architecture disagreement.
- [x] **Task 11.2.2 - Preserve independent review**
  - [x] **Sub-task 11.2.2.1:** Prevent the agent that authored a correction from being its sole final reviewer.
  - [x] **Sub-task 11.2.2.2:** If Codex performs an emergency correction, require Claude or a human to review it before closure.

**Story acceptance criteria**

- [x] **AC 11.1:** Given pass, repeated finding, contradictory finding, optional suggestion, and unresolved disagreement fixtures, when the loop runs, then every outcome is bounded and truthful.
- [x] **AC 11.2:** Given three failed correction rounds, when the limit is reached, then no fourth autonomous round starts.

### Sprint 11 Gate

- [x] **Gate 11.1:** Finding lifecycle, correction binding, escalation, and disagreement tests pass. See `docs/evidence/sprint-11.md`.
- [x] **Gate 11.2:** No agent is permitted to author and solely approve the same material correction.

---

## Sprint 12 - Durable Journal, Checkpoints, and Recovery

**Sprint goal:** Preserve continuity across process crashes, context limits, provider limits, restart, and power loss.

### Story 12.1 - Append-Only Event Journal

- [x] **Task 12.1.1 - Define journal records**
  - [x] **Sub-task 12.1.1.1:** Include sequence, schema version, prior-event hash, timestamp, run/task/repository identities, event kind, outcome, and evidence references.
  - [x] **Sub-task 12.1.1.2:** Canonically serialize and hash each record.
  - [x] **Sub-task 12.1.1.3:** Bound record size and reject unknown critical fields.
  - [x] **Sub-task 12.1.1.4:** Redact sensitive values before persistence without storing removed content.
- [x] **Task 12.1.2 - Implement durable append**
  - [x] **Sub-task 12.1.2.1:** Use atomic create/append and explicit flush policy.
  - [x] **Sub-task 12.1.2.2:** Detect truncation, corruption, reordering, duplication, and chain breaks.
  - [x] **Sub-task 12.1.2.3:** Prevent concurrent writers through exact lock ownership.

### Story 12.2 - Snapshots and Resume

- [x] **Task 12.2.1 - Maintain current-state snapshot**
  - [x] **Sub-task 12.2.1.1:** Derive snapshots from accepted journal events.
  - [x] **Sub-task 12.2.1.2:** Write through temporary file, flush, and atomic replacement.
  - [x] **Sub-task 12.2.1.3:** Verify snapshot hash and journal position on load.
- [x] **Task 12.2.2 - Reconcile live state after restart**
  - [x] **Sub-task 12.2.2.1:** Reconcile repository, worktree, branch, commit, process, agent session, gate, and evidence identities.
  - [x] **Sub-task 12.2.2.2:** Resume only idempotent or explicitly recoverable phases.
  - [x] **Sub-task 12.2.2.3:** Mark uncertain effects visibly and require re-observation before action.
  - [x] **Sub-task 12.2.2.4:** Never replay a state-changing provider or Git action merely to discover whether it happened.

**Story acceptance criteria**

- [x] **AC 12.1:** Given torn writes and every single-field journal mutation, when recovery loads state, then corruption is identified at the exact record and no action resumes.
- [x] **AC 12.2:** Given crashes at every orchestration transition, when CodingMage restarts, then it resumes safely or stops with an exact blocker and no duplicated Git or provider effect.

### Sprint 12 Gate

- [x] **Gate 12.1:** Journal mutation, concurrent writer, snapshot, and crash-recovery campaigns pass. See `docs/evidence/sprint-12.md`.
- [x] **Gate 12.2:** Recovery requires no target-source copy or raw provider transcript in durable state.

---

## Sprint 13 - Monitoring and Operator Controls

**Sprint goal:** Make progress, limitations, and intervention controls visible from a VS Code terminal.

### Story 13.1 - Status Model and Event Stream

- [x] **Task 13.1.1 - Define status schema**
  - [x] **Sub-task 13.1.1.1:** Include target, task, state, agent, model, branch, commit, command, gate, findings, correction count, elapsed time, pause, and blocker fields.
  - [x] **Sub-task 13.1.1.2:** Mark unavailable usage and reset information as unknown rather than zero.
  - [x] **Sub-task 13.1.1.3:** Provide stable JSON and human-readable renderings.
- [x] **Task 13.1.2 - Stream ordered events**
  - [x] **Sub-task 13.1.2.1:** Stream lifecycle events with sequence and correlation IDs.
  - [x] **Sub-task 13.1.2.2:** Bound update rate and coalesce noisy command output.
  - [x] **Sub-task 13.1.2.3:** Reconnect a monitor without affecting the running coordinator.

### Story 13.2 - Operator Commands

- [x] **Task 13.2.1 - Implement read-only controls**
  - [x] **Sub-task 13.2.1.1:** Add `status`, `explain-blocker`, `open-diff`, `open-log`, and `doctor`.
  - [x] **Sub-task 13.2.1.2:** Ensure read-only commands cannot acquire mutation grants.
- [x] **Task 13.2.2 - Implement lifecycle controls**
  - [x] **Sub-task 13.2.2.1:** Add `pause`, `resume`, `stop-after-unit`, and `cancel`.
  - [x] **Sub-task 13.2.2.2:** Authenticate control requests to the same local user and exact run.
  - [x] **Sub-task 13.2.2.3:** Make repeated commands idempotent.
  - [x] **Sub-task 13.2.2.4:** Preserve recovery state after cancellation.

**Story acceptance criteria**

- [x] **AC 13.1:** Given a running fake workflow, when monitors attach, disconnect, and reattach, then they reconstruct identical current status without changing execution.
- [x] **AC 13.2:** Given repeated or stale control commands, when received, then only the exact active run can change state once.

### Sprint 13 Gate

- [x] **Gate 13.1:** VS Code terminal rendering, JSON status, reconnect, and control tests pass. See `docs/evidence/sprint-13.md`.
- [x] **Gate 13.2:** Monitoring exposes no credential, hidden reasoning, or unnecessary source content.

---

## Sprint 14 - Background Service, Quotas, and Scheduling

**Sprint goal:** Run unattended as an unprivileged Fedora user service without busy loops or orphaned work.

### Story 14.1 - User Service Lifecycle

- [x] **Task 14.1.1 - Define systemd user service**
  - [x] **Sub-task 14.1.1.1:** Generate a user-owned service with explicit executable, configuration, state root, restart policy, and resource limits.
  - [x] **Sub-task 14.1.1.2:** Do not install system services, request root, or enable lingering automatically.
  - [x] **Sub-task 14.1.1.3:** Add install, verify, start, stop, and uninstall commands with previews.
- [x] **Task 14.1.2 - Enforce single ownership**
  - [x] **Sub-task 14.1.2.1:** Hold one coordinator lock per target repository.
  - [x] **Sub-task 14.1.2.2:** Detect stale lock owners without adopting unrelated processes.
  - [x] **Sub-task 14.1.2.3:** Release locks and descendants on normal stop, failure, logout, and shutdown.

### Story 14.2 - Provider Capacity and Backoff

- [x] **Task 14.2.1 - Normalize capacity signals**
  - [x] **Sub-task 14.2.1.1:** Parse exposed token usage, cost, rate-limit, remaining-capacity, and reset data without relying on message text alone where structured fields exist.
  - [x] **Sub-task 14.2.1.2:** Represent unavailable metrics explicitly.
  - [x] **Sub-task 14.2.1.3:** Detect authentication expiry separately from quota exhaustion.
- [x] **Task 14.2.2 - Implement pause and retry**
  - [x] **Sub-task 14.2.2.1:** Pause until a known reset with bounded jitter.
  - [x] **Sub-task 14.2.2.2:** Use capped exponential backoff when reset is unknown.
  - [x] **Sub-task 14.2.2.3:** Persist retry count and next attempt across restarts.
  - [x] **Sub-task 14.2.2.4:** Stop after configured terminal authentication or provider failures.

**Story acceptance criteria**

- [ ] **AC 14.1:** Given service restart, logout simulation, crash, and duplicate launch, when lifecycle handling runs, then exactly one coordinator owns the target and no child survives incorrectly.
- [x] **AC 14.2:** Given quota, network, authentication, overload, and malformed provider errors, when classified, then CodingMage pauses, retries, or stops according to exact policy without spinning.

### Sprint 14 Gate

- [ ] **Gate 14.1:** User-service install/uninstall and lifecycle tests pass in an isolated Fedora user session. Local unit generation, native parsing, filesystem lifecycle, duplicate ownership, crash release, and cleanup policy pass; a real isolated login/logout session remains external evidence. See `docs/evidence/sprint-14-local.md`.
- [x] **Gate 14.2:** A sustained fake-provider run demonstrates bounded retry and clean recovery. See `docs/evidence/sprint-14-local.md`.

---

## Sprint 15 - GitHub Issues and Pull Requests

**Sprint goal:** Synchronize reviewable story-level work with GitHub without making GitHub metadata the authority for local execution.

### Story 15.1 - Authenticated GitHub Adapter

- [x] **Task 15.1.1 - Define GitHub capability boundary**
  - [x] **Sub-task 15.1.1.1:** Use authenticated `gh` or an approved API adapter without reading raw tokens.
  - [x] **Sub-task 15.1.1.2:** Bind account, host, owner, repository, branch, and pull-request identities.
  - [x] **Sub-task 15.1.1.3:** Require explicit configuration for issue read/write, pull-request read/write, comments, and branch push.
  - [x] **Sub-task 15.1.1.4:** Deny merge, release, repository settings, secrets, Actions configuration, and destructive administration.
- [x] **Task 15.1.2 - Handle uncertain network effects**
  - [x] **Sub-task 15.1.2.1:** Assign idempotency keys to issue, comment, and pull-request operations.
  - [x] **Sub-task 15.1.2.2:** Reconcile remote state after timeout rather than blindly replaying writes.
  - [x] **Sub-task 15.1.2.3:** Record redirects, host changes, authentication changes, and permission reductions.

### Story 15.2 - Story Issues and Draft Pull Requests

- [x] **Task 15.2.1 - Synchronize story issues**
  - [x] **Sub-task 15.2.1.1:** Create one issue per configured story with canonical source links and sub-task checkboxes.
  - [x] **Sub-task 15.2.1.2:** Update only CodingMage-owned issue sections.
  - [x] **Sub-task 15.2.1.3:** Preserve human comments, labels, assignments, and edits.
  - [x] **Sub-task 15.2.1.4:** Never treat issue checkbox edits as local completion evidence.
- [x] **Task 15.2.2 - Create and update draft pull requests**
  - [x] **Sub-task 15.2.2.1:** Push only the exact authorized feature branch after local gates pass.
  - [x] **Sub-task 15.2.2.2:** Create a draft PR with scope, commits, tests, findings, limitations, and blockers.
  - [x] **Sub-task 15.2.2.3:** Add Codex findings as structured review output without impersonating human approval.
  - [x] **Sub-task 15.2.2.4:** Update the PR after correction and final verification.
  - [x] **Sub-task 15.2.2.5:** Leave readiness, approval, and merge to configured human policy.

**Story acceptance criteria**

- [x] **AC 15.1:** Given timeouts, duplicate delivery, redirects, permission loss, and concurrent human edits, when synchronization runs, then effects are idempotent and human-owned content survives.
- [x] **AC 15.2:** Given a locally verified story, when publication is enabled, then one draft PR points to the exact branch and evidence while no merge or protected-branch effect occurs.

### Sprint 15 Gate

- [ ] **Gate 15.1:** Fake GitHub server and authorized test-repository campaigns pass. The fake-server campaign passes; an authenticated disposable test-repository campaign remains external evidence. See `docs/evidence/sprint-15-local.md`.
- [x] **Gate 15.2:** GitHub integration can be disabled completely without affecting local orchestration. See `docs/evidence/sprint-15-local.md`.

---

## Sprint 16 - Security and Adversarial Hardening

**Sprint goal:** Prove hostile repositories, agents, processes, outputs, and concurrent activity cannot escape CodingMage's authority boundaries.

### Story 16.1 - Prompt and Output Hostility

- [x] **Task 16.1.1 - Build hostile-content corpus**
  - [x] **Sub-task 16.1.1.1:** Seed instructions in source, comments, filenames, task text, issues, logs, test output, and model responses requesting broader authority.
  - [x] **Sub-task 16.1.1.2:** Seed fabricated pass claims, commits, tests, reviews, blockers, and quota notices.
  - [x] **Sub-task 16.1.1.3:** Seed oversized, malformed, reordered, duplicated, and future-version JSON events.
- [x] **Task 16.1.2 - Verify containment**
  - [x] **Sub-task 16.1.2.1:** Assert no hostile content changes policy, routing, command templates, repository scope, or task state.
  - [x] **Sub-task 16.1.2.2:** Assert sensitive canaries do not enter prompts, logs, evidence, GitHub content, or diagnostics without explicit policy.

### Story 16.2 - Git, Filesystem, and Process Attacks

- [ ] **Task 16.2.1 - Exercise repository attacks.** Every local create, commit, review, cleanup, and recovery case passes; authenticated push and network recovery remain external. See `docs/evidence/sprint-16-local.md`.
  - [x] **Sub-task 16.2.1.1:** Test hooks, aliases, filters, drivers, helpers, pagers, editors, signers, URL rewrites, alternates, replacement refs, malicious objects, submodules, LFS, unsafe ownership, case collisions, and Unicode collisions.
  - [ ] **Sub-task 16.2.1.2:** Test active-checkout changes during create, commit, review, cleanup, push, and recovery. Deterministic local create, commit, review, cleanup, path-replacement, and recovery campaigns pass; authenticated push and network recovery await external credentials.
  - [x] **Sub-task 16.2.1.3:** Test renamed, deleted, replaced, and overlapping repository/worktree paths.
- [x] **Task 16.2.2 - Exercise process attacks**
  - [x] **Sub-task 16.2.2.1:** Test parent, child, and grandchild crashes, hangs, output floods, descriptor inheritance, and process-name collisions.
  - [x] **Sub-task 16.2.2.2:** Test unrelated matching processes and units appearing during cleanup.
  - [x] **Sub-task 16.2.2.3:** Assert cleanup acts only on proven owned identities.

### Story 16.3 - State and Evidence Attacks

- [x] **Task 16.3.1 - Mutate durable records**
  - [x] **Sub-task 16.3.1.1:** Mutate identity, sequence, hash chain, repository, task, branch, commit, model, outcome, and evidence fields independently.
  - [x] **Sub-task 16.3.1.2:** Test rollback, replay, truncation, concurrent writer, and stale snapshot attacks.
  - [x] **Sub-task 16.3.1.3:** Assert no corrupted record authorizes a new effect.

**Story acceptance criteria**

- [x] **AC 16.1:** Given the complete hostile-content corpus, when every boundary processes it, then authority remains unchanged and no synthetic secret leaks.
- [x] **AC 16.2:** Given concurrent local repository and process attacks, when CodingMage operates or recovers, then user state survives and cleanup touches only proven owned artifacts.
- [x] **AC 16.3:** Given every single-field evidence mutation, when verification runs, then the exact corruption blocks advancement.

### Sprint 16 Gate

- [x] **Gate 16.1:** Hostile-content, Git, filesystem, process, state, and evidence campaigns pass locally. Authenticated network cases remain separately open under sub-task `16.2.1.2`.
- [x] **Gate 16.2:** Manual fuzzing remains a separate explicit gate and is not claimed by deterministic mutation tests. Execution remains open as `External 5`.

---

## Sprint 17 - Soak Testing and AgentMage Pilot

**Sprint goal:** Demonstrate sustained reliable operation before CodingMage receives unattended access to real AgentMage development work.

### Story 17.1 - Disposable Soak Campaign

- [x] **Task 17.1.1 - Build disposable target fixtures**
  - [x] **Sub-task 17.1.1.1:** Create small Rust, Python, JavaScript, documentation-only, dirty, conflicted, and malformed-plan repositories.
  - [x] **Sub-task 17.1.1.2:** Create fake Claude, Codex, GitHub, quota, network, sleep, crash, and restart schedules.
- [ ] **Task 17.1.2 - Run sustained operation**
  - [ ] **Sub-task 17.1.2.1:** Execute repeated implementation-review-correction cycles for at least 24 continuous hours.
  - [ ] **Sub-task 17.1.2.2:** Extend to 48 hours after correcting every blocking reliability defect.
  - [ ] **Sub-task 17.1.2.3:** Inject provider limits, network loss, process crashes, service restarts, malformed output, stale commits, concurrent user changes, and operator controls.
  - [ ] **Sub-task 17.1.2.4:** Verify zero duplicate tasks, skipped gates, false completions, orphan processes, unowned mutations, and unbounded storage growth.

### Story 17.2 - Controlled AgentMage Pilot

- [x] **Task 17.2.1 - Prepare AgentMage authorization.** See `docs/evidence/sprint-17-agentmage-preflight.md`.
  - [x] **Sub-task 17.2.1.1:** Review AgentMage instructions, branch policy, task structure, commands, blockers, and current clean checkpoint.
  - [x] **Sub-task 17.2.1.2:** Create a minimal disposable CodingMage target configuration with network, GitHub mutation, and publication disabled.
  - [x] **Sub-task 17.2.1.3:** Select a low-risk read-only patch-transfer preview fixture mapped to AgentMage sub-task `42.1.1.7` without modifying the real checkout.
- [ ] **Task 17.2.2 - Run supervised pilot units**
  - [x] **Sub-task 17.2.2.1:** Complete one dry run using fake agents against a source-free AgentMage-shaped disposable fixture; exact lifecycle order and zero repository mutation are asserted in `codingmage-soak`.
  - [ ] **Sub-task 17.2.2.2:** Complete one live Claude implementation and Codex review on a disposable AgentMage branch.
  - [ ] **Sub-task 17.2.2.3:** Complete five supervised bounded units with no authority or recovery defect.
  - [ ] **Sub-task 17.2.2.4:** Enable background execution only after explicit owner approval of retained evidence.
- [ ] **Task 17.2.3 - Run unattended pilot**
  - [ ] **Sub-task 17.2.3.1:** Run one story through implementation, review, correction, and checkpoint without operator intervention.
  - [ ] **Sub-task 17.2.3.2:** Pause correctly on every external blocker and quota event.
  - [ ] **Sub-task 17.2.3.3:** Produce a complete handoff and status report for human inspection.

**Story acceptance criteria**

- [ ] **AC 17.1:** Given the 48-hour disposable campaign, when evidence is reviewed, then every injected interruption is recovered or blocked exactly and no uncontrolled residue exists.
- [ ] **AC 17.2:** Given the AgentMage pilot, when human review compares repository state and CodingMage claims, then every commit, test, finding, limitation, and blocker is accurate.

### Sprint 17 Gate

- [ ] **Gate 17.1:** Disposable soak campaign passes after the last reliability correction.
- [ ] **Gate 17.2:** AgentMage unattended operation requires explicit product-owner approval and remains disabled until granted.

---

## Sprint 18 - Packaging and Cross-Platform Foundations

**Sprint goal:** Package a reproducible Linux release and prepare truthful macOS and Windows adaptation without claiming unexecuted evidence.

### Story 18.1 - Fedora Release Package

- [x] **Task 18.1.1 - Produce reproducible binary**
  - [x] **Sub-task 18.1.1.1:** Pin dependencies and verify license/provenance inventory.
  - [x] **Sub-task 18.1.1.2:** Build release artifacts from a clean checkout twice and compare outputs.
  - [x] **Sub-task 18.1.1.3:** Generate SBOM, checksums, build manifest, and installation layout.
  - [x] **Sub-task 18.1.1.4:** Verify no credentials, logs, target source, local paths, or debug-only authority enter the package.
- [ ] **Task 18.1.2 - Installation and removal.** Binary lifecycle passes; packaged user-service lifecycle remains open. See `docs/evidence/sprint-18-linux-package.md`.
  - [x] **Sub-task 18.1.2.1:** Install under user-owned paths without root.
  - [ ] **Sub-task 18.1.2.2:** Install, verify, start, stop, upgrade, rollback, and remove the user service.
  - [x] **Sub-task 18.1.2.3:** Preserve user configuration and state only according to explicit retention policy.

### Story 18.2 - macOS and Windows Design

- [x] **Task 18.2.1 - Define platform adapter contracts**
  - [x] **Sub-task 18.2.1.1:** Separate filesystem identity, process containment, service management, credential references, and monitoring behind platform interfaces.
  - [x] **Sub-task 18.2.1.2:** Document Linux evidence as Linux-only.
- [ ] **Task 18.2.2 - Implement macOS adapter**
  - [ ] **Sub-task 18.2.2.1:** Implement Apple Silicon process, filesystem, launch-agent, and keychain-reference behavior.
  - [ ] **Sub-task 18.2.2.2:** Run native macOS lifecycle, Git safety, provider, monitoring, and recovery tests on supported hardware.
- [x] **Task 18.2.3 - Plan Windows adapter**
  - [x] **Sub-task 18.2.3.1:** Define native Windows process, job-object, NTFS identity, service/task, credential, and console requirements.
  - [x] **Sub-task 18.2.3.2:** Keep Windows support explicitly unimplemented until native evidence exists.

**Story acceptance criteria**

- [x] **AC 18.1:** Given two clean Fedora builds, when artifacts are compared and installed, then identities are reproducible and removal leaves only explicitly retained user data.
- [x] **AC 18.2:** Given platform documentation and packages, when reviewed, then no Linux result is represented as macOS or Windows evidence.

### Sprint 18 Gate

- [ ] **Gate 18.1:** Fedora package, install, upgrade, rollback, removal, SBOM, and provenance gates pass.
- [x] **Gate 18.2:** macOS and Windows claims remain blocked until native execution evidence exists.

---

## Sprint 19 - Reusable Project Adapters and v1 Readiness

**Sprint goal:** Generalize the proven AgentMage workflow into a reusable tool without weakening target-specific safety.

### Story 19.1 - Project Adapter Contract

- [x] **Task 19.1.1 - Support multiple task sources**
  - [x] **Sub-task 19.1.1.1:** Retain Markdown `TASKS.md` as the reference adapter.
  - [x] **Sub-task 19.1.1.2:** Add optional GitHub Issue task-source adapter with local canonical snapshot.
  - [x] **Sub-task 19.1.1.3:** Define future Jira and Azure DevOps adapters without implementing them in v1 unless separately approved.
- [x] **Task 19.1.2 - Support repository-specific gates**
  - [x] **Sub-task 19.1.2.1:** Define typed Rust, Python, Node, documentation, and custom literal-command profiles.
  - [x] **Sub-task 19.1.2.2:** Require each project to declare expected artifacts, test tiers, and prohibited operations.
  - [x] **Sub-task 19.1.2.3:** Prevent one project's configuration, state, credentials, sessions, and evidence from crossing into another.

### Story 19.2 - Operational Readiness

- [x] **Task 19.2.1 - Complete documentation**
  - [x] **Sub-task 19.2.1.1:** Add installation, quickstart, configuration, security, recovery, monitoring, GitHub, model-routing, and troubleshooting guides.
  - [x] **Sub-task 19.2.1.2:** Add executable disposable example repositories and sanitized walkthroughs.
  - [x] **Sub-task 19.2.1.3:** Document every unsupported action and platform.
- [ ] **Task 19.2.2 - Complete release review**
  - [ ] **Sub-task 19.2.2.1:** Run all local unit, integration, security, recovery, performance, packaging, and documentation gates.
  - [ ] **Sub-task 19.2.2.2:** Complete an independent code and threat-model review.
  - [ ] **Sub-task 19.2.2.3:** Resolve or explicitly accept every open risk.
  - [ ] **Sub-task 19.2.2.4:** Create a signed release candidate without publishing it automatically.
- [x] **Task 19.2.3 - Evaluate bootstrap retirement**
  - [x] **Sub-task 19.2.3.1:** Compare CodingMage capabilities with AgentMage's current source-level orchestration implementation.
  - [x] **Sub-task 19.2.3.2:** Decide whether CodingMage remains independent, becomes a thin client, or is retired.
  - [x] **Sub-task 19.2.3.3:** Preserve migration and archival instructions before any retirement.

**Story acceptance criteria**

- [x] **AC 19.1:** Given two unrelated authorized repositories, when CodingMage coordinates them sequentially and concurrently, then their authority, state, worktrees, sessions, and evidence remain isolated.
- [ ] **AC 19.2:** Given the v1 candidate, when a new user follows the documented workflow, then installation, diagnosis, supervised execution, pause, recovery, and removal succeed without undocumented authority.

### Sprint 19 Gate

- [ ] **Gate 19.1:** All locally required release gates pass with current immutable evidence.
- [x] **Gate 19.2:** Native platform, provider, independent-review, and manual-test limitations are stated truthfully.
- [x] **Gate 19.3:** Publication requires an explicit human release decision. No release was published.

---

## External and Deferred Evidence Register

These items must remain open until their prerequisites actually exist:

- [ ] **External 1:** Native macOS implementation and execution evidence on supported Apple Silicon hardware.
- [ ] **External 2:** Native Windows implementation and execution evidence on supported Windows hardware.
- [ ] **External 3:** Authenticated GitHub issue and pull-request tests against an explicitly approved test repository.
- [ ] **External 4:** Independent security and architecture review by a qualified human reviewer.
- [ ] **External 5:** Manual fuzzing campaign after the deterministic attack corpus is stable.
- [ ] **External 6:** Explicit product-owner approval before unattended AgentMage operation.
- [ ] **External 7:** Explicit product-owner approval before any public release or package publication.

## Immediate Next Unit

The first dependency-ready implementation unit is:

- [ ] **Next 1:** Execute one explicitly authorized disposable live Claude task to verify the implemented file-only authority profile, redaction boundary, coordinator-owned commit, and malicious-instruction refusal without broadening credentials or publication authority.
