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

## Open

- CM-R07.2 notices crosswalk; CM-R07.3 and CM-R07.4 are human-only or live qualification.
