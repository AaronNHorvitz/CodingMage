# Sprint 16 Local Evidence

## Scope

This evidence covers deterministic hostile-content, repository, filesystem, process, durable-state, and evidence mutation campaigns on Fedora. It does not claim authenticated network publication concurrency or manual fuzz execution.

## Hostile Content

- Source-, comment-, filename-, task-, issue-, log-, test-output-, and model-shaped instructions remain bounded provider progress data.
- Fabricated commit, test, merge, release, blocker, review, and quota claims do not receive coordinator authority.
- Duplicate, reordered, skipped, wrong-session, malformed, oversized, unterminated, unknown-field, and future-version events fail closed.
- Provider output and source content are represented in durable state only by redaction categories. Raw provider text, source content, credentials, and hidden reasoning are not journal fields.
- GitHub ownership-marker injection is rejected and operator diagnostics remain typed and content-minimized.

## Repository and Filesystem

- Hardened Git inspection clears ambient environment, disables prompts, hooks, replacement objects, credential helpers, external diffs, pagers, editors, optional locks, and user protocols.
- Repository aliases, helpers, filters, drivers, includes, pagers, editors, signers, URL rewrites, alternates, replacement refs, active hooks, submodules, LFS configuration, case collisions, and non-ASCII path collisions are classified unsafe.
- Execution canaries and a loopback URL-rewrite listener prove hostile configuration is not invoked during inventory.
- An unreachable malformed loose object remains inert and byte-identical.
- Unsafe owner identity is rejected by the repository authorization boundary.
- Renamed or replaced target and worktree paths fail identity validation. Dirty user state, unrelated refs, notes, stashes, tags, configuration, index bytes, and concurrent active-checkout file writes survive owned-worktree lifecycle operations.

## Process and State

- Literal arguments have no shell or response-file interpretation; environment and working directory are explicit.
- Timeout, cancellation, output flood, process-count excess, parent loss, child loss, and grandchild cleanup terminate only the exact owned process group.
- A concurrent unrelated process using the identical executable survives cleanup.
- Child processes inherit neither coordinator control files nor ambient environment.
- Every durable identity, sequence, schema version, prior hash, repository, task, event, outcome, evidence, redaction, and record-hash mutation is rejected.
- Duplicate, reordered, torn, replayed, chain-broken, stale-snapshot, forged-snapshot, concurrent-writer, and abandoned-temporary-write cases fail closed.
- Recovery derives exact identities from durable records and never authorizes a new state-changing effect from an uncertain record.

## Verification

Implementation commits: `df000e5`, `88f8010`, `e1f5f38`, `6596628`, and `9c70c2d`.

```text
cargo test --workspace --all-targets
passed: 141 tests; 0 failed

cargo clippy --workspace --all-targets --all-features -- -D warnings
passed

cargo fmt --all -- --check
passed

python3 scripts/check_architecture.py
passed

python3 scripts/docs_check.py
passed

python3 -m unittest discover -s tests -v
passed: 8 tests; 0 failed

git diff --check
passed
```

The focused Git campaign was rerun after the full workspace command and passed 17 tests with zero failures.

## Open Evidence

Sub-task `16.2.1.2`, `AC 16.2`, and `Gate 16.1` remain open because CodingMage does not yet have locally executable commit, review, authenticated push, and network-recovery composition through which to inject concurrent active-checkout mutations. Existing create, inventory, cleanup, filesystem replacement, and recovery campaigns pass, but they are not a substitute for that later end-to-end operation.

Manual fuzzing remains the separately deferred `External 5` gate. Deterministic mutation and hostile-corpus tests do not claim fuzz coverage.
