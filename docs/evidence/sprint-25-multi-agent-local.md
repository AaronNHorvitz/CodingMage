# Sprint 25 Multi-Agent Local Evidence

## Boundary

This record covers deterministic local, fake-provider, fake-publication, local-process, clean-clone,
and unsigned Linux-package evidence for the multi-agent campaign implementation. It does not claim
authenticated provider or GitHub qualification, long-duration execution, native macOS or Windows
evidence, independent human review, package signing, release publication, or valuable-target
approval.

The machine-checkable mapping is
[`multi-agent-scenario-matrix.json`](multi-agent-scenario-matrix.json). Every required scenario maps
to existing implementation and executable test symbols. The matrix validator rejects missing,
duplicate, reordered, malformed, or dangling mappings and independently scans tracked content for
the prohibited private project identifier. The separate
[`multi-agent-evidence-binding.json`](multi-agent-evidence-binding.json) binds the implementation,
test, schema, fixture, package inputs, gate-command set, reproducible archive, and platform claim.
Mutation tests prove that drift in each claim class fails closed.

## Local Commands

The release and clean-clone qualification is bound to source commit
`51533f2ce59619a2e33b9570c85c3782190e47aa`. The current evidence-binding refresh is commit
`4967f6d`. The following commands were run locally:

```bash
python3 -m unittest tests.test_multi_agent_matrix
cargo test -p codingmage-campaign --lib
cargo test -p codingmage-runtime --lib
cargo test -p codingmage-cli --test workflow parallel_campaign_runs_five_pods_and_serializes_reviewed_integrations -- --exact --nocapture
cargo test -p codingmage-cli --test workflow serial_campaign_advances_two_reviewed_tasks_without_touching_active_checkout -- --exact
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
python3 scripts/check_architecture.py
python3 scripts/docs_check.py
python3 -m unittest discover -s tests -p 'test_*.py'
python3 -m unittest tests.test_release_tools -v
python3 scripts/package_release.py --output <first-output>
python3 scripts/package_release.py --output <second-output>
python3 scripts/install_release.py <install-or-lifecycle-action> --prefix <temporary-prefix>
git diff --check
```

## Results

| Command | Exit | Result |
| --- | --- | --- |
| `cargo test --workspace --all-targets` | `0` | 323 passed, 0 failed, 1 ignored guarded qualification. |
| Exact process-backed five-pod workflow | `0` | 1 passed, 0 failed, 60.98 seconds. |
| Guarded sustained five-pod workflow | `0` | 1 passed, 0 failed, 2 complete internal cycles, 121.97 seconds. |
| `cargo clippy --workspace --all-targets -- -D warnings` | `0` | No warnings. |
| `cargo fmt --all -- --check` | `0` | No formatting drift. |
| `python3 scripts/check_architecture.py` | `0` | Dependency policy passed from the clean clone. |
| Clean-clone Python discovery | `0` | 20 passed, 0 failed at the package source commit. |
| Current Python discovery | `0` | 28 passed, 0 failed after the current package and policy refresh. |
| `python3 -m unittest tests.test_multi_agent_matrix` | `0` | 5 passed, 0 failed. |
| `python3 -m unittest tests.test_release_tools -v` | `0` | 3 passed, 0 failed. |
| `python3 scripts/docs_check.py` | `0` | Documentation checks passed. |
| `cargo test -p codingmage-plan --lib` | `0` | 10 passed, including canonical repository-plan parsing. |
| Two independent package invocations | `0` | Byte-identical Linux x86-64 archives. |
| Installed-package lifecycle | `0` | Install, verify, version execution, upgrade, verify, rollback, verify, and remove passed under a temporary rootless prefix. |
| `git diff --check` | `0` | No whitespace errors. |

The scenario-matrix SHA-256 at this baseline is
`02961fdbbae9995c350c1def63a2fb50f7e1a5399bb0611a319f01cb11cee499`. Both package invocations
produced `codingmage-0.1.0-linux-x86_64.tar.gz` with SHA-256
`d3e38d513b03567af5d7bfc58cba16260650a9af9566b9a276745610a8696f4d`. The archive contains the
binary, checksums, source-bound build manifest, SPDX 2.3 SBOM, license, readme, and security policy.
It is an unsigned local candidate, not a published release.

The clean clone was unchanged after qualification. Strict Clippy produced no warnings. The ordinary
workspace run ignored one credential-gated sustained qualification by design; that case was then
run explicitly at its minimum two-cycle bound. Documentation checking covered local links, Mermaid
declarations, unsupported claims, and configured secret patterns. Both required case-insensitive
repository scans returned zero prohibited-name matches.

The sustained five-pod qualification is separately guarded and ignored during ordinary test runs:

```bash
CODINGMAGE_SUSTAINED_SOAK=approved CODINGMAGE_SUSTAINED_SOAK_CYCLES=2 \
  cargo test -p codingmage-cli --test workflow sustained_five_pod_campaign_qualification \
  -- --exact --ignored --nocapture
```

The guarded command passed at its minimum two-cycle bound after the last local concurrency test
correction. This is current local multi-pod soak evidence, not a claim of long-duration,
authenticated-provider, native-platform, independently reviewed, or valuable-target qualification.
A sustained one-pod production run remains open, so the combined sustained rollout gate remains
open.

## External Qualification

[`qualify_campaign.py`](../../scripts/qualify_campaign.py) provides a guarded live runner. It
requires absolute ordinary files, a clean repository at the campaign's immutable starting commit,
deny-first destination policy, and mode-consistent publication. It runs `doctor`, captures a
source-free `campaign-preflight` report, and stops before model inference. A later invocation runs
the campaign only when both the exact manually approved report digest and external-effects
acknowledgment are supplied. Capability probes rely on the provider CLIs' existing login stores;
they do not read or persist credential values.

Authenticated provider, GitHub, CI, and integration evidence remains open until an operator supplies
an authorized disposable target and executes the guarded command. No ordinary unit, integration, or
CI command can cross that boundary accidentally.
