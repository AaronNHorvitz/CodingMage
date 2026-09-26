# CM-R07 Support Export - Local Evidence

- **Status:** Local implementation with a real-process test; no release, human or live claim.
- **Work package:** CM-R07.1 (CAP-47)
- **Source revision:** the commit that carries this document

## CM-R07.1 - Manual redacted support bundle

`export_support_bundle` (`crates/codingmage-runtime/src/support.rs`) and the `support-bundle`
command write, on explicit request only, a new private directory containing the redacted
configuration view, campaign status, blocker explanation, final report and mission status as the
existing commands expose them, plus `manifest.json` (coordinator version, campaign and repository
identities, authority digest, per-file SHA-256 and size, the list of absent records, `redacted`
and `uploaded: false`) and a `README.txt`. An existing output directory or a relative or escaping
path is refused with `codingmage.runtime.spec`. The command has no network capability and reads
nothing but the durable records.

Test (`cargo test -p codingmage-cli --test campaign_mission support_bundle`): a never-started
campaign exports only its configuration view and lists the four absent records; a second export
into the same directory is refused; after a hands-off mission campaign the bundle contains status
and mission records whose manifest digests and sizes match the files, and no file contains charter
prose, private markers or sandbox paths.

## CM-R07.2 - Licensing, notices and packaging metadata crosswalk

Observed on the current locked dependency graph (`cargo metadata --locked --offline`):

| Item | Observation | Status |
| --- | --- | --- |
| Root license | `LICENSE` is Apache-2.0 and `Cargo.toml` declares `license = "Apache-2.0"` for every workspace crate | Consistent |
| SPDX and package metadata | `scripts/package_release.py` emits an SPDX 2.3 SBOM with `licenseConcluded` from crate metadata and packages `LICENSE` and `THIRD-PARTY-NOTICES.md`; the release scanner requires both files | Mechanism present; a candidate cannot be built here (CM-R01.6 external review record) |
| Dependency licence metadata | 413 third-party packages, all with a licence expression; the dominant expressions are `MIT OR Apache-2.0`, `MIT`, `Apache-2.0 OR MIT`, `Apache-2.0`, `Zlib` combinations, `Unlicense OR MIT` and two `BSL-1.0` (Boost Software License, permissive, unrelated to the Business Source License); `r-efi` and `self_cell` offer a permissive option in their `OR` expressions | Every package permits an admitted licence; no copyleft-only dependency |
| Model obligations | No model weights are bundled or downloaded; provider CLIs are operator-installed | No obligation carried by this repository |
| Contribution provenance | Commits carry the owner or the agent co-author line; `CONTRIBUTING.md` records the pre-release policy | Present |
| Per-crate notice text | `THIRD-PARTY-NOTICES.md` describes the mechanism but enumerates no crate, and the packager copies no per-crate licence or copyright text; MIT, BSD and Zlib require reproducing their notices with a binary distribution | Gap: new row CM-R07.2a |

Outcome: one genuine gap (per-crate notice text in the package) is recorded as row CM-R07.2a;
licensing changes remain the owner's separate decision and nothing here is a release claim.

## Open

- CM-R07.2a per-crate notice generation at packaging; CM-R07.3 and CM-R07.4 are human-only or
  live qualification.
