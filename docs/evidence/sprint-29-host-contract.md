# Sprint 29 Host Contract Evidence (Sub-task 29.1.1.1)

- **Status:** Closed versioned request, capability, status, and control schemas
  defined in `crates/codingmage-contracts/src/host.rs`, plus the bounded
  transport definition in `crates/codingmage-contracts/src/transport.rs`;
  fakes and policy subjection remain open under Sub-tasks 29.1.1.3–29.1.1.4
- **Source:** `2045e5e39a9903709a84faf1a36d8eb060b5dca8` on `muse/complete-development`
- **Executed:** 2026-09-20 on Fedora Linux x86-64

## Boundary

Local preparation only under the readiness census in
`sprint-29-readiness-census.md` and the explicit dependencies 4.2.1.1,
12.2.1.1, and 19.1.2.3 (all checked). These schemas prove the consumer
contract shape; fakes prove nothing live. No host is admitted, no transport
exists, no coordinator operation is connected, and no live integration,
cross-repository work, or publication is authorized.

## Schemas

- `HostRequest`: `protocol_version`, `client_id`, `repository`, optional
  `run`/`task`, `source_commit`, `task_source_digest`,
  `authority_policy_digest`, `request_id`, `expected_state_revision`,
  `operation`. Unknown fields, foreign versions, and malformed commits or
  digests fail; identities reuse the validated `ClientId`, `RepositoryId`,
  `RunId`, `TaskId`, and `RequestId` newtypes.
- `HostCapability`: version plus a non-empty, duplicate-free operation set
  bounded by `MAX_HOST_OPERATIONS`.
- `HostControl`: request identity, target run, revision, and one of the four
  closed control operations; a control against any other revision reports
  `StaleRevision` instead of cancelling newer state.
- `HostStatus`: request identity, run, lifecycle, revision, and a blocker
  category present exactly when the lifecycle is blocked.
- Stable codes: `codingmage.host.invalid_request`,
  `codingmage.host.unsupported_version`, `codingmage.host.stale_revision`.

## Transport (Sub-task 29.1.1.2)

Selection: local Unix-domain socket under an operator-owned `0700`
directory, JSON payloads, four-byte big-endian length-prefixed frames
(`TRANSPORT_KIND = "unix-socket-json-v1"`). No shell-string transport, no
terminal scraping, no shared mutable file, no upstream endpoint. Socket
input/output attaches in a later stage against exactly these types.

- `TransportLimits`: maximum frame bytes (1 KiB–16 MiB, default 1 MiB),
  per-connection request ceiling, and read/write deadlines (100 ms–10 min),
  all validated by `verify()`.
- `encode_frame`/`decode_frame`: pure length-prefixed codec. Empty payloads,
  oversize advertisements, truncated buffers, and zero lengths each fail with
  a distinct typed error; trailing bytes are left for later frames.
- `validate_peer_directory`: admits only the expected owner uid with zero
  group/other permission bits; the input/output layer supplies the observed
  metadata.
- Stable codes: `codingmage.host.transport.invalid_limits`,
  `empty_frame`, `oversize_frame`, `truncated_frame`, `malformed_frame`,
  `peer_refused`.

## Commands

```text
cargo fmt -p codingmage-contracts -- --check
cargo clippy -p codingmage-contracts --all-targets -- -D warnings
cargo test -p codingmage-contracts --all-targets -- --test-threads=1
python3 scripts/verification_inventory.py --write
python3 scripts/verification_inventory.py
git diff --check
```

## Results

- `cargo fmt --check`: clean.
- `cargo clippy --all-targets -- -D warnings`: no warnings.
- `cargo test --all-targets`: 27 passed, 0 failed (7 pre-existing, 11
  schema fixtures, 9 transport fixtures covering limits, framing, peer
  refusal, and code stability).
- Verification inventory regenerated: 1,226 surfaces became 1,269 with zero
  new explicit gaps; the generator check passes.

## Open Items

Sub-tasks 29.1.1.3 (fake clients and fixtures) and 29.1.1.4 (coordinator
policy subjection with recorded schema identities) remain open in dependency
order. Consumer-side qualification in Story 29.3 additionally needs the
blocked frozen-target soak and evidence renewal.
