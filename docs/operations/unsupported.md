# Unsupported Behavior

The current build does not claim:

- Release-qualified authenticated Claude implementation or Codex review evidence across the final test matrix.
- Authenticated GitHub issue, comment, branch-push, or draft-pull-request evidence.
- A passing prescribed ten-outcome production soak campaign.
- A passing one-pod, local-only ten-task controlled-target campaign with complete reconciliation.
- Lead-side deferred and human-decision outcomes with deterministic reconsideration.
- Parallel live coding pods.
- Native macOS execution, lifecycle, packaging, Keychain, or provider evidence.
- Native Windows execution, job-object, NTFS, service/task, credential, console, or packaging evidence.
- Independent human security or architecture approval.
- Manual fuzz execution.
- Signed or published packages or releases.
- Merge, release, destructive Git, paid-resource, or external-infrastructure authority.
- Jira or Azure DevOps adapters.

The CLI `run` command fails closed until live composition gates are satisfied. Local unit and accelerated tests are not represented as native, authenticated, sustained-duration, independent-review, or fuzz evidence.
