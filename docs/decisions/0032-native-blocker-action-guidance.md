# ADR 0032: Native blocker action guidance

- **Status:** Accepted
- **Date:** 2026-09-29
- **Decision owners:** CodingMage implementation

## Context

The coordinator publishes closed, content-free reason and trigger codes. The first Blockers
view displayed those codes but used one manual-evidence instruction for every pending deferral.
The coordinator accepts `campaign-observe-trigger` for `provider_reset`,
`review_completion` and `operator_resume` only. Campaign-head advancement, path-lease release
and gate-resource release are coordinator-observed events. Directing an operator to submit an
evidence command for those three events would describe an action the backend refuses.

## Decision

The native view maps the published blocker, deferral-trigger and human-decision codes to
bundled, contextual recovery text. Unknown codes, mismatched reason/trigger pairs and unknown
trigger states receive neutral inspection guidance. The three externally observable triggers
explain the operator-supplied
request identity and evidence digest; the three coordinator-observed triggers explicitly say
that the operator command cannot mark them. Satisfied triggers are shown as already recorded.
The view remains read-only. It does not infer an evidence digest, submit a clearance request,
make a human decision or advance a campaign. The coordinator keeps all authority and
revalidation.

## Consequences

This copy is a client projection of the coordinator's current closed code contract. When that
contract adds a code, the neutral fallback remains safe until the native catalogue is updated.
The complete section-5 state/depth/action matrix and human accessibility checks remain open.
