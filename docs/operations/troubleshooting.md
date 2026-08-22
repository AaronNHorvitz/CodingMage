# Troubleshooting

## Start With Read-Only Inspection

Run `doctor`, `campaign-status`, `campaign-report`, and, when applicable,
`campaign-explain-blocker`. Preserve state and retained branches. These commands expose typed
identities, lifecycle, utilization, and content-free reasons without mutating the campaign.

## Common Team-Campaign Conditions

| Condition | Meaning | Response |
| --- | --- | --- |
| No pod starts | No dependency-ready nonconflicting task fits current leases or resource ceilings. | Inspect blockers, active pods, path/resource declarations, and limits; do not invent work. |
| Provider circuit open | Repeated classified transient failures reached the configured threshold. | Wait for the configured cooldown or restore provider service; do not loop restarts. |
| CI correction paused | Required CI is failed, stale, ambiguous, or bound to another commit. | Verify exact PR, commit, and check identities; preserve the existing PR. |
| Integration refused | Head, reviewed SHA, paths, gates, review, or transfer no longer matches. | Inspect the retained candidate and campaign report; never force or reset. |
| Promotion pending | Safe default requires an exact human destination decision. | Review the final report and use `campaign-approve-destination` only for the displayed immutable identities. |
| Qualification refused | Acknowledgment, file, clean-head, publication, destination, or mode preflight failed. | Correct the declared precondition; do not weaken the qualification script. |

## Stable CLI Codes

- `codingmage.cli.usage`: malformed or incomplete command syntax.
- `codingmage.cli.invalid_argument`: relative, linked, missing, or otherwise invalid selected path.
- `codingmage.cli.config`: configuration failed strict loading.
- `codingmage.cli.repository`: repository authorization or hardened inventory failed.
- `codingmage.cli.plan`: task source failed strict parsing.
- `codingmage.cli.no_ready_work`: no open dependency-ready sub-task exists.
- `codingmage.cli.refused`: initialization would overwrite or broaden authority.
- `codingmage.cli.execution_unavailable`: requested composition is unavailable under current
  authority.

Errors omit source text, path values, credentials, and provider output. Report the code, current
source commit, command shape with sensitive values removed, and relevant content-minimized evidence
digest.
