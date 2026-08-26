# Migration And Upgrade

## Pre-Release Rule

There is no general in-place state migration contract before the first release. Configuration,
journal, checkpoint, campaign, and provider schemas are integrity boundaries; a new binary must
refuse an unknown or incomplete shape instead of guessing how to upgrade it.

## Safe Upgrade Procedure

1. Stop after a completed unit and record the exact binary, configuration, repository, campaign,
   branch, head, task-source, state-root, and scratch-root identities.
2. Preserve the old binary and make a private backup of configuration and runtime state without
   copying credentials or target source into the CodingMage repository.
3. Read the candidate release notes, compatibility matrix, and any version-specific migration
   instructions.
4. Install the new candidate with the rootless installer, which retains one prior binary.
5. Run package verification, version output, configuration validation, `doctor`, and read-only
   status before resuming work.
6. If validation fails, stop. Roll back the binary and preserve the rejected state for diagnosis;
   do not edit integrity-bound files manually.
7. Remove the prior binary only after the upgraded campaign and retention checks pass.

## Data Retention

Normal removal preserves configuration and runtime state. `--purge-data` is a separate destructive
decision and must not be combined with an upgrade or rollback attempt. Release notes must identify
any state that cannot be read by the prior binary.
