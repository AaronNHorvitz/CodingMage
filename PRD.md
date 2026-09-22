# CodingMage Product Requirements

## Status and Product Purpose

CodingMage is a local engineering-team coordinator. The owner supplies an authorized objective
and constraints; CodingMage plans bounded work, coordinates implementation pods, verifies and
independently reviews their changes, integrates accepted candidates, and reports actual outcomes.
The product must support an owner who participates frequently and an owner who leaves the entire
authorized engineering campaign running without further interaction.

This PRD consolidates existing product intent and the planned extension accepted in
[Decision 0014](docs/decisions/0014-owner-optional-engineering-team.md).
It does not claim that the new director, involvement modes or no-intervention qualification exist.
[TASKS.md](TASKS.md) is the canonical implementation/evidence ledger;
[the development plan](PRODUCT-DEVELOPMENT-PLAN.md) orders delivery without creating another queue.
Existing code and evidence are starting assets, not automatic qualification for the new modes.

## Audience and Outcomes

The primary user is a repository owner who wants a complete engineering workflow rather than a
collection of agent terminals. They should be able to:

- authorize one objective, target repository, task source and bounded campaign;
- choose their involvement separately from publication and destination-promotion authority;
- delegate technical decisions without repeatedly forwarding messages or approving routine steps;
- observe useful delivered behavior, exact blockers and remaining limits at any time;
- pause, revoke, cancel or resume without losing user work or replaying uncertain effects; and
- receive a tested, independently model-reviewed candidate and truthful delivery disposition.

Success is accepted functionality with traceable evidence, not tokens consumed, agent activity,
manager messages, checked boxes or model assertions. CodingMage remains a standalone product;
a host UI, optional memory service and additional provider adapters are not required for this loop.

## Native Linux Desktop Interface

The owner selected a native Linux desktop app for CodingMage. This is a planned first-class
product surface, not a web dashboard, a terminal launcher or a separate orchestration system.
Its purpose is to make the existing coding workflow usable without managing agent terminals.
The opening screen is the actual workspace, not a marketing page or fabricated demonstration.

The first milestone works with CodingMage's existing local backend and supported providers.
It does not require a host application, external memory service, additional stack component or
completion of Sprints 32 through 35. Those later capabilities extend the same interface when ready.
Opening the app or a repository must not start agents, edit task status or authorize a campaign.

| ID | Required behavior | Planned task |
| --- | --- | --- |
| CM-UI-001 | Native Linux presentation over the existing coordinator, with one authoritative state and permission model. | 36.1.1 |
| CM-UI-002 | Repository workspace with searchable tasks, dependencies, actual team activity, blockers and distinct source-checkbox versus verified-outcome states. | 36.1.2 |
| CM-UI-003 | Guided configuration of supported provider/model profiles, target, limits and local campaign policy, with actionable readiness errors. | 36.1.3 |
| CM-UI-004 | Explicit campaign admission, start/pause/resume/stop/cancel and truthful reconnect/recovery behavior; no silent authority expansion. | 36.2.1 |
| CM-UI-005 | Inspectable changes, exact-commit review and test results, bounded activity, and exportable outcome reports from real backend records. | 36.2.2 |
| CM-UI-006 | Native user-workflow, keyboard/accessibility, resizing, resource-use and installation verification with separate live-provider qualification. | 36.2.3 |

The interface must keep completed work, accepted outcomes, blocked/deferred work, source checkboxes
and delivery state separate. Unknown usage stays unknown; disconnected or stale observations must
not appear live. Empty, loading, failure and recovery states must be usable, not replaced with
sample success data. Repository text and model output never grant permission.

Owner involvement remains independent of publication authority. Show supervised, exception-only
and hands-off options only when the corresponding backend capability is available; explain an
unavailable mode without pretending it works. Once qualified, hands-off requires one initial
authorization and no routine UI approvals. Observer disconnect is not campaign cancellation;
the implementation must make execution ownership and supported detach/reconnect behavior explicit.
Existing provider authentication and human-only gates remain unchanged.

## Owner Involvement Modes

These are planned product modes, not currently accepted configuration keys.

| Mode | Owner interaction | Undelegated decision |
| --- | --- | --- |
| Supervised | Owner participates at configured checkpoints. | Ask through the authenticated operator path; unrelated permitted work can continue. |
| Exception-only | Routine decisions are delegated; only declared exceptions request attention. | Create one deduplicated request, hold affected work, continue independent work. |
| Hands-off | One initial authorization; no mid-campaign question, approval dialog or terminal input is required. | Apply a predeclared permitted disposition or retain an exact blocked/deferred outcome; continue other work. |

Hands-off is a first-class delivery requirement, not an instruction to auto-answer prompts.
The preflight must reject an impossible unattended configuration, such as mandatory per-task owner
approval without a noninteractive disposition. Missing logins, provider terms or unavailable
capabilities must be detected before launch where possible. If they change during execution,
record a typed hold or use an already qualified authorized alternative; never acquire authority.

The owner can disconnect after admission. Observer disconnect is not revocation. Explicit revocation,
expiry, pause and cancellation still stop or narrow execution under the recorded policy.
No mode can create human review, signing material, credentials or missing evidence.

## Delegation and Delivery

An immutable mission charter binds the objective, acceptance criteria, exclusions, repository and
source identities, approved decision domains, architecture constraints, path/command policies,
role/provider profiles, integration/delivery targets, independent limits, expiry and revocation.
Authority is issued by the authenticated operator mechanism, not by repository text or an agent.

Delegable engineering choices can include decomposition, sequencing, implementation approach,
approved dependency selection, architecture decisions and public-interface evolution within
explicitly authorized domains. Record rationale and impacted requirements. The director may
propose wider scope, but cannot add it to the charter, erase acceptance criteria, expand limits,
weaken gates, rewrite policy or turn a blocked result into approval.

Owner involvement and delivery authority are independent:

| Delivery | Required authority |
| --- | --- |
| Local candidate and campaign integration | Exact configured repository/worktree and integration grants. |
| Feature-branch push or draft PR | Separate exact remote, branch, account and publication grants. |
| Automatic destination promotion | Explicit eligible destination policy, current candidate-bound checks and separately qualified implementation; never implied by hands-off mode. |
| Signing, package release, deployment or infrastructure changes | Existing separate owner and external gates; not granted by this extension. |

The initial hands-off milestone ends at a local campaign candidate. Later remote delivery is
optional. A campaign can finish its authorized engineering work while destination promotion is
withheld; report those two dispositions separately. If the authorized objective requires a blocked
delivery or an unmet acceptance criterion, report it incomplete, not as successful delivery.
Existing provider sessions may be used within approved token/process/time limits. Buying credits,
new subscriptions, resources or credentials is not a delegated fallback.

## Team and Authority

| Role | Work product | Boundary |
| --- | --- | --- |
| Product owner | Mission charter and permitted policy | Retains ownership and revocation; need not remain online. |
| Director | Milestone priorities, outcome evaluation and bounded replanning proposals | Cannot grant authority or attest completion. |
| Team lead | Dependency-ready work packets, interface coordination and pod assignments | Proposals require coordinator validation. |
| Pod implementer | Changes inside exact leased paths in one worktree | Cannot control Git, gates, canonical status or publication. |
| Independent reviewer | Findings on an exact cumulative commit and evidence | Read-only; no approval of its own contributions. |
| QA and specialists | Product acceptance, security, performance, accessibility or design findings | Risk-triggered, bounded roles, not a new source of authority. |
| Deterministic coordinator | Validated state transitions, scheduling, leases, limits and recovery | Existing Rust coordinator and journal remain authoritative. |
| Integration/verifier services | Exact commits, configured checks and candidate integration | Serialize effects and revalidate current identities. |

Roles are provider-neutral contracts, not permanent processes or a requirement for meetings between
agents. Invoke specialists only when required; use deterministic checks before expensive review.
Provider/version/model capabilities and minimum review strength must be qualified per role.
A separate reviewer session is required, and differing vendors alone do not establish independence.

## Functional Requirements and Traceability

| ID | Required behavior | Planned task |
| --- | --- | --- |
| CM-TEAM-001 | Versioned mission charter and immutable owner delegation with deny-first validation. | 32.1.1 |
| CM-TEAM-002 | Supervised, exception-only and hands-off involvement independent of delivery policy. | 32.1.2 |
| CM-TEAM-003 | Noninteractive decision disposition, bounded retry and exact no-progress handling. | 32.2.1 |
| CM-TEAM-004 | Revocation, expiry and restart preserve narrowed authority and uncertain effects. | 32.2.2 |
| CM-TEAM-005 | Director evaluates milestones and reprioritizes only approved work. | 33.1.1 |
| CM-TEAM-006 | Team lead produces dependency-safe packets with complete acceptance coverage. | 33.1.2 |
| CM-TEAM-007 | Qualified role/provider routing and independent review/correction separation. | 33.2.1 |
| CM-TEAM-008 | On-demand QA/specialists assess outcomes without acquiring writer authority. | 33.2.2 |
| CM-TEAM-009 | Interface-aware scheduling and stale-base integration protect concurrent work. | 34.1.1 |
| CM-TEAM-010 | Independent product acceptance runs real workflows on the integrated candidate. | 34.1.2 |
| CM-TEAM-011 | Source-bound handoffs and inspectable decision history survive limited context. | 34.2.1 |
| CM-TEAM-012 | Useful outcome, quality, blocker and resource metrics drive bounded replanning. | 34.2.2 |
| CM-TEAM-013 | One-pod and then staged live multi-pod qualification with zero owner responses. | 35.1.1 |
| CM-TEAM-014 | Fault, authority, cancellation and restart campaigns retain truthful results. | 35.1.2 |
| CM-TEAM-015 | Optional preauthorized remote delivery uses exact identities and independent gates. | 35.2.1 |
| CM-TEAM-016 | Accurate setup, controls, evidence, independent review and completion reporting. | 35.2.2 |

Every requirement inherits positive, negative, boundary, malformed-input, failure/recovery and
side-effect tests where applicable. New acceptance must preserve serial operation and existing
provider, platform, source-privacy, owner-approval and independent human-review boundaries.

## Acceptance and Quality

The no-intervention campaign must run through actual installed coordinator/provider boundaries,
not direct helper calls or scripted model prose alone. Declare the workload, expected outcomes,
resource ceilings, latency and throughput criteria before execution. Record every trial and failure.

The local hands-off fixture includes ten accepted outcomes, with at least five genuine integrated
implementation outcomes, a failed test and repair, review-driven correction, an unavailable
external dependency with continued independent progress, owner disconnect and durable restart.
No campaign may count ten blockers as ten completed tasks. Explicitly reconcile accepted outcomes,
completed tasks, acceptance coverage, interventions and delivery state. Existing soak requirements
remain separate and are not retrospectively satisfied by this new fixture.

No unauthorized writes, escaped descendants, repeated uncertain external effects, suppressed
failures, unreviewed integrations, false completion, silent model-strength reduction or prompt-based
authority are permitted. Token counts may be unknown when the provider cannot report them; retain
that fact and enforce independent process/time/output limits rather than inventing usage.

## Non-Goals and Current Limits

No self-modification of CodingMage during a target campaign, unrestricted agent shell, automatic
policy expansion, mandatory external memory, new orchestration engine, or simulation of human
approval. Release gates and human-only decisions are not delegated to a model by changing mode.
This planning increment changes no executable, schema, enabled provider, running session or gate
result. Sprints 32 through 36 remain unchecked until their exact evidence exists. The native UI
is assigned to a future implementation agent; this documentation update does not build or launch it.
