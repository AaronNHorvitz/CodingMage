# ADR 0065: Revalidate Setup Authority at Publication

- **Status:** Accepted for local correction; independent re-review pending
- **Date:** 2026-10-01
- **Decision owners:** CodingMage implementation

## Context

The first independent review of the public Setup authorization command found
that a repository HEAD change during private candidate staging could still
leave a public file and a success receipt. The command checked authority after
reading stdin, while the guarded file writer still had work to do before its
publication syscall. The review reproduced this with a disposable repository
and a delayed candidate sync. The original finding remains retained privately.

## Decision

The report writer accepts an optional fallible authority check for a caller
whose result depends on repository state. It runs after candidate validation
and destination checks, immediately before either create-only link or explicit
overwrite exchange. The Setup authorization caller passes the held
`RepositoryAuthorization::revalidate` check and maps failure before publication
to `codingmage.cli.stale_observation`. The private candidate is removed when
that refusal occurs; an existing destination is left intact.

The writer checks the same authority after publication and before returning
success. If authority changed in that interval, a public effect may already
exist. It returns `codingmage.cli.uncertain_write`, retains the private stage
for reconciliation and emits no success receipt. The existing destination
identity and exact-byte checks still run around publication. Other report
writer callers use a no-op authority check; their existing contracts do not
claim the Setup record's repository HEAD binding.

These checks bound the command's observations at the publication boundary.
They cannot make a Git HEAD change atomic with a filesystem syscall or promise
that HEAD remains unchanged after the command returns. The ordinary concurrent
change tested by the reviewer is covered at the two checks; separate
same-UID and privileged parent mutation limits remain as stated in Decision
0063. No new dependency or publication authority is added.

## Verification

The deterministic test changes a disposable fixture's Git HEAD immediately
before and immediately after publication, for both no-overwrite and overwrite.
Before publication it requires stale refusal and an untouched public leaf;
after publication it requires uncertain-write and retained reconciliation
material. The focused result and cumulative checks are in
[the Setup command evidence](../evidence/sprint-36-setup-command-boundary.md).
Independent exact-commit re-review is required before this correction is
treated as accepted. Human, live-provider and release gates remain separate.
