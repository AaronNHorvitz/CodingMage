# Ecosystem Integration Requirements

## Status And Authority

This is the accepted planning contract under
[Decision 0013](../decisions/0013-optional-ecosystem-integration.md). It defines future behavior;
no host interface, USTE adapter or Muse worker support is implemented or qualified by this document.
The README remains the product overview and [TASKS.md](../../TASKS.md) remains the canonical
implementation sequence. This document supplies acceptance requirements, not a second task queue.

"Host application" is the public role name for the separately developed application that may later
present and request CodingMage workflows. The actual operator mapping stays outside this repository
under the existing downstream-name privacy rule. A host interface is distinct from using CodingMage
to edit an explicitly authorized target repository; neither grants access to a sibling checkout.

## Product Boundaries

| Component | Responsibility | Authority it does not gain |
| --- | --- | --- |
| Host application | Present authorized jobs, progress, blockers and operator controls | Direct writes to CodingMage state, arbitrary commands, policy expansion or inferred approval |
| CodingMage | Enforce admission, scheduling, worktree ownership, gates, review, recovery and configured publication | Self-modification, unauthorized target edits, human-only approvals or release publication |
| USTE context consumer | Retrieve and, when separately granted, store bounded project context | Task completion, review approval, command authority or canonical execution-state ownership |
| Provider adapter, including future Muse | Translate supported structured requests and observations | Git/test orchestration, uncontrolled workers, self-approval or direct publication |

Effective authority is the intersection of the operator's grants, CodingMage policy and the exact
request scope. A client may narrow that scope; neither the client nor a model may broaden it.
No component shares writable journals, provider sessions or credential stores with another product.
CodingMage remains useful when every optional integration is disabled.

## Host Job And Control Contract

### Admission And Identity

The future versioned envelope must bind protocol/schema version, client identity, repository
identity, campaign/run/task identity, source commit, canonical task-source digest, authority-policy
digest, unique request ID, expected state revision and requested operation. Operations and payloads
must use closed schemas; unknown fields, incompatible versions and mismatched identities fail
before acquiring a lease or starting a process.

A submitted job references preauthorized local configuration and task authority. Source text,
repository paths from untrusted payloads and model prose cannot become executable commands or
authorization. Version and capability discovery exposes only supported operations. No shell-string
transport, terminal scraping, shared mutable file or undocumented provider transcript is permitted.

The first implementation must select and document a local process or authenticated local IPC
transport, including peer validation, bounded framing, deadlines and cancellation. Do not invent
an upstream endpoint or imply that this requirement specifies an existing host API.

### Operations And Recovery

The bounded surface is job submission, read-only status/report/event observation, pause, resume,
stop-after-unit and cancel, subject to the existing operation-specific grants. Approval, blocker
clearance and destination-promotion requests must preserve their existing exact human or operator
evidence requirements; a user-interface button or provider result is not an approval artifact.

Duplicate identical request IDs return the already recorded disposition. Reusing an ID with a
different payload or authority fails closed. Admission and effects follow durable intent and
postcondition reconciliation; transport success or retry does not promise exactly-once execution.
After either side restarts, uncertain effects are reconciled before any replay. Stale controls
must not cancel a newer run or adopt another client's process.

Status and events are versioned, ordered and bounded, with a resumable cursor and explicit gaps.
They expose task IDs, lifecycle, outcome, evidence references and truthful blocker categories,
without source, prompts, credentials, raw provider output or hidden reasoning. Disconnect behavior
is explicit: observer loss does not silently grant new execution, erase cancellation or claim
completion. The local CLI remains available if the host is disconnected.

### First Qualified Workflow

An explicitly admitted client submits one bounded job against a disposable target, observes its
progress, and receives a deterministically verified, independently reviewed exact candidate.
Disconnect, duplicate submission, cancellation and host/coordinator restart are injected. The
active target checkout and destination branch remain unchanged; no duplicate candidate or external
effect is created. USTE and Muse support are not prerequisites for this milestone.

## Optional USTE Context Contract

### Context And Canonical State

USTE is an optional source of contextual memory, not a replacement for the append-only journal and
atomic snapshot required by [Decision 0004](../decisions/0004-append-only-journal-atomic-snapshot.md).
Canonical task authority, execution state, reviews and evidence remain independently verifiable
without a memory query. No journal dual-writing, state migration or shared writable database is
authorized here.

Each operation binds an authorized repository/owner namespace, caller capability, schema and
interface version, operation ID, bounded query/result sizes, timeout and provenance. Returned
context includes the source identity, revision or content digest, freshness and authorization scope
needed to reject wrong-project, revoked, stale or untraceable records. Context is untrusted input;
instructions inside it cannot grant tools, broaden paths, satisfy a gate or clear a blocker.

Read and write operations are separate grants. Writes require idempotency, retention/deletion
policy and an explicit content-minimization rule. Credentials, hidden reasoning, unrestricted
provider logs and canonical authority records must not be exported as general memory. Any source
content requires a separately documented data policy and explicit authorization.

### Availability And Compatibility

With optional context disabled or unavailable, the existing standalone workflow continues from its
authoritative local inputs. A task explicitly requiring a memory capability remains truthfully
blocked if that capability is absent; it must not silently pass using fabricated or stale results.
Bounded retry must not create a busy loop or postpone cancellation indefinitely.

Implement consumer-side schemas and fakes locally first. Before implementing or enabling a live
USTE binding, require admission in both roadmaps and a pinned, committed interface with tested
operations. Never depend on a sibling's dirty tree or change its branch or processes. Record the
exact consumer and provider revisions, schema, policy, platform and test evidence. The bounded M1
pilot may satisfy only matching operations and acceptance cases; it does not qualify all of USTE.

## Optional Muse Provider Contract

Using Muse Code to develop this repository is an external development workflow under
[CONTRIBUTING.md](../../CONTRIBUTING.md), not a product capability. It must not start CodingMage
against its own source or rewrite a running coordinator. Development agents use isolated worktrees,
respect the current task order and preserve sibling repositories and processes.

Runtime support, if pursued, follows [Decision 0003](../decisions/0003-cli-adapter-provider-boundary.md)
and the existing provider-neutral operations: probe, start, continue, cancel and usage observation.
First establish the actual installed CLI's supported structured protocol, exact executable/version,
session semantics and capabilities; this plan prescribes no unverified Muse command or flag.
Unsupported behavior or version drift must fail admission. No provider SDK or terminal scraping
may become an alternate authority path.

Every nested worker and its filesystem access, worktree, process lifetime, retries and resource
usage must remain inside coordinator ownership and aggregate limits. Provider-managed Git,
publication, tools and background agents must be disabled or confined to the granted contract; if
that cannot be demonstrated, the provider remains unsupported. Cancellation must account for all
owned descendants and preserve unrelated processes. Provider success is still only an untrusted
claim; deterministic gates and a separate qualified reviewer decide candidate acceptance.

Authentication uses the established reference-only provider boundary. Installing a provider,
purchasing a plan, creating credentials or conducting paid qualification is not authorized by a
planning checkbox. Live evidence needs actual compatible access and the existing opt-in authority.

## Required Consumer Acceptance Matrix

| Surface | Positive case | Required refusal, failure and recovery cases |
| --- | --- | --- |
| Host job admission | Exact authorized job reaches a reviewed candidate | Unknown schema, wrong repository, stale policy, widened scope, duplicate/conflicting ID, missing approval |
| Host control/status | Exact owned run responds to bounded controls and resumes observation | Unauthorized peer, stale revision, cross-client cancel, event gaps, disconnect and crash around each effect |
| USTE context | Authorized bounded context with provenance is returned | Wrong namespace, revoked access, stale result, hostile instructions, malformed/oversized output, quota, outage and timeout |
| USTE writes | Separately granted bounded write is recorded once per operation | Retry after uncertain outcome, cross-project identity, forbidden content and retention/deletion failure |
| Muse adapter | Supported structured candidate survives independent verification | Version drift, malformed output, quota, session mismatch, worker escape, orphan, cancellation and false success |
| Cross-product workflow | One disposable job survives interruption and returns exact evidence | Duplicate effects, leaked state/credentials, unrelated checkout/process mutation and unsupported completion claims |

Each test binds the immutable source/artifact versions, schema and authority digests, configured
limits, platform, command, result and limitations. Fakes prove the consumer contract only. Actual
integration requires consumer-side tests against the pinned provider; native, live-provider,
independent-review and release claims retain their separate gates.

## Scheduling And Shared Desktop Limits

Sprints 29, 30 and 31 are separate optional packages. Standalone release work stays first. If every
remaining first-release item is bound to an actual unavailable prerequisite, a dependency-ready
local contract/fake package may proceed; record the readiness census and return to earlier work
when its prerequisite is satisfied. Merely writing this document does not close implementation.

Run one heavy workload per development session, one Cargo build job and one test thread. Check
combined RAM/swap headroom before heavy work alongside other agents. Use a systemd user scope with
MemoryHigh=5G, MemoryMax=6G and MemorySwapMax=512M where supported, verify the actual workload's
cgroup, and retain any stricter repository limit. Do not launch parallel heavy workers, local
inference or VMs without established combined headroom; inability to enforce a limit is a blocker.
Terminate only exactly owned processes. A resource failure never authorizes removing limits or
lowering acceptance criteria.

Provider-internal concurrency is included in the budget. Separate repositories do not make shared
RAM, CPU, disk or test ports independent. No integrated deployment, cross-repository write, runtime
replacement or release publication is part of this planning increment.
