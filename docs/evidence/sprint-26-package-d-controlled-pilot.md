# Sprint 26 Package D Controlled Pilot

**Date:** 2026-08-26

**Packaged source:** `04627990b4a2e2ea6d2c8cbc9cdcae13ea97bd82`

**Scope:** Sub-task `26.1.3.6` and acceptance criterion `26.4`

## Candidate Identities

| Item | SHA-256 |
| --- | --- |
| External candidate-construction review record | `d6b4e39cb6f9c6a380006c221dd801c45b634382d65e4023949b5cf4bfd040d0` |
| Linux x86-64 archive | `8643bb20195259a4f7424abac3bb2200876d48c15b38819ef3348dce39b78045` |
| Tracked-source archive | `ffbdb2afd96412cedb444a5bba869279ff9c4813dd180a5590198055fd60c7b2` |
| Installed binary | `47324d665a4a8ece63ef7abbed047057f8757fd6ea8f42dafd41ce2d8ecd5296` |
| Supervised configuration | `fcef1ee9253b6debef18494a2562ae6e73d159e969236947c89e2b3a8b7a77a5` |
| Supervised run specification | `e26ef409390216da846cd60286cdf316a3c8ba97a24e3f1ffd12748947f9d01b` |

Package D was built from a clean exact source through the external review-record preflight. The
packager completed two separate locked release builds, rejected no identity, and emitted the binary
archive, tracked-source archive, SBOM, dependency inventory, provenance, manifests, notices, and
checksums. The rootless installer verified the archive and installed binary. Installed `--version`,
top-level `--help`, and `run --help` matched the tested source contract.

## Fresh Supervised Unit

The installed binary selected one exact synthetic task in a fresh clean local-only repository. The
configuration denied network, push, issues, pull requests, task merge, destination merge, and
publication. Its literal gates were `git diff --check` and byte-for-byte artifact comparison.

The run completed in 42 seconds:

- run: `run-1e88663bb27786a636cfcc8449c7f320`;
- candidate: `84dc972ded6934eff52532afd15c53b6c6df65e0`;
- completion: `15ddaf743b122522d754748b998eb75e3705ce0f`;
- review: `pass`;
- correction rounds: `0`;
- provider attempts: `2`; and
- process invocations: `11`.

The active synthetic checkout stayed clean at its original commit. The retained branch contained
the exact artifact and only the mechanically reconciled task checkbox. The owned worktree was
removed, scratch was empty, and no qualification provider process remained.

## Unattended Outcomes

The installed Package D binary then ran the established local-only unattended harness without
operator controls during execution:

1. A live Claude implementation completed one useful unit, received one deterministic independent
   review correction, passed repeated gates, integrated only the isolated campaign branch, and
   reconciled its checkpoint in 20 seconds.
2. Six deterministic external-prerequisite cases stopped as blocked with the exact reason retained,
   the task unchecked, one lead invocation, and no downstream provider call.
3. Lead, implementer, and reviewer quota cases stopped as bounded capacity pauses at their exact
   provider boundary without retry loops or false completion.

Across all ten unattended cases, publication was local-only, active checkouts were preserved,
provider attempts were bounded, checkpoints reconciled, pod worktrees were released, and retained
campaign worktrees remained Git-bound for inspection or resume.

## Disposition And Limits

This evidence closes the fresh installed supervised and three-outcome controlled-target requirement
in Sub-task `26.1.3.6`. Combined with the correction timeout, exact resume, immutable review retry,
and durable attempt-ledger evidence, it also closes `AC 26.4`.

It does not close Sub-task `26.1.7.3`, which separately requires the final installed candidate
against the established frozen no-hardlinks target. It does not satisfy the approximately ten-task
controlled soak, final candidate build, native platform, authenticated GitHub, manual fuzz,
independent human review, signing, or publication gates. Package D is retained as qualification
evidence and is not represented as a signed or published release.
