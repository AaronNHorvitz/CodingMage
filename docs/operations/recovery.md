# Recovery

## Durable Rule

CodingMage uses append-only integrity-bound events and atomically replaced snapshots. Every
provider, process, Git, gate, review, publication, CI, integration, completion, control, and
promotion effect records durable intent before execution and one observation afterward.

Restart never trusts the snapshot alone. It revalidates repository, campaign head, task source,
worktree, branch, commit, process, session, model, lease, resource, issue, PR, CI, policy, and
evidence identities against actual local or remote state. Contradiction fails closed.

## Team Campaign Recovery

- An admitted initial batch interrupted before provider intent receives fresh bounded run and
  reservation identities.
- An initial batch interrupted after provider intent reuses the exact recorded run and resource
  identities.
- Already observed worktree, session, candidate, gate, review, and completion events replay
  idempotently without repeating their effects.
- CI correction reuses its exact task lineage and reservation only when every identity still
  matches.
- Publication uncertainty is reobserved by idempotency key before any retry.
- Integration intent is reconciled against actual campaign ancestry and completion state before
  applying, observing, or refusing one effect.
- Promotion intent is bound to the final report, destination head, final commit, PR, policy, and
  operator decision.

Healthy sibling pods continue when one pod crashes or times out. Cancellation terminates only
proven owned descendants. Leases are released only after terminal reconciliation, and uncertain
candidates remain available for diagnosis.

## Operator Procedure

1. Preserve state, scratch worktrees, retained branches, and the target checkout.
2. Run `codingmage doctor` with the exact configuration.
3. Run `codingmage campaign-status` and `codingmage campaign-report` with the exact campaign file.
4. Resolve only the reported external prerequisite or identity mismatch.
5. Restart the same `codingmage campaign` command; do not construct a replacement campaign spec to
   bypass refusal.

Deleting state to bypass a refusal destroys evidence and is unsupported. Never manually adopt,
reset, merge, or remove a retained branch or worktree merely because its name resembles an owned
identity.
