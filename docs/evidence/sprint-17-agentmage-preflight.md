# Sprint 17 AgentMage Preflight Evidence

- **Status:** Preparation and fake-agent dry run complete; live and unattended pilots remain open
- **AgentMage checkpoint:** branch `claude/sprint-41-verification`, commit `caf22663`
- **CodingMage implementation commit:** `a1327a3`
- **Executed:** 2026-08-19 on Fedora Linux with Rust 1.95.0

## Read-Only Inventory

The real AgentMage checkout was clean and synchronized when inspected. Its current task source,
root documentation, security policy, Rust workspace, Node scripts, branch, and recent commit were
read without mutation. AgentMage's current dependency-ready local direction is Sprint 42; the
remaining read-only patch-transfer or merge-back preview portion of sub-task `42.1.1.7` is the
narrowest usable pilot analogue. No claim is made that CodingMage selected work directly from
AgentMage's normative task file.

AgentMage's checklist intentionally uses a richer legacy identifier and heading grammar than the
strict CodingMage Markdown task-source adapter. CodingMage refused that source with
`codingmage.cli.plan`. The parser was not weakened and AgentMage was not reformatted.

## Disposable Preflight

A no-hardlink local disposable clone was created from the exact checkpoint. A separate canonical
one-unit pilot plan was added and committed only inside that clone. The generated CodingMage
configuration used local-only publication and denied network, push, issue, and pull-request
capabilities. `codingmage doctor` reported the clone clean and ready, and `codingmage plan`
selected the exact patch-preview fixture sub-task. The real AgentMage checkout remained unchanged.

The committed `codingmage-soak` integration test makes the reusable dry run source-independent:

- materialize a clean AgentMage-shaped documentation fixture containing no AgentMage source;
- parse and select one canonical read-only preview unit;
- execute claim, fake implementation, local verification, fake senior review, final verification,
  checkpoint, completion reconciliation, and release in exact order; and
- compare Git `HEAD` and porcelain state before and after, requiring zero repository mutation.

## Verification

The focused `codingmage-soak` unit and integration suites pass. Strict Clippy, formatting, and
`git diff --check` also pass for the pilot implementation.

## Open Conditions

The production `codingmage run` command remains disabled. No live Claude or Codex invocation,
credential access, network operation, push, merge, issue, pull request, or AgentMage source change
occurred. The live supervised units, five-unit campaign, unattended pilot, and product-owner
approval therefore remain open.
