# Sprint 36 offline Help and About increment

The native shell now has a Help and About destination, reachable by navigation and Ctrl+8
without a repository or coordinator executable. It explains the supported setup and campaign
path, common recovery steps, keyboard controls and a small glossary. It includes the
repository's Apache-2.0 source licence and third-party source notices as bundled text; opening
either document makes no network request. The screen offers Overview and Setup navigation.
An explicit Copy diagnostic summary action shows and copies only the UI version, platform and
sibling executable availability; it excludes binary and repository paths, source content,
prompts and credentials. The unavailable-binary test checks that its private path is omitted.

The Help screen also has a manual diagnostic action over the existing `support-bundle` CLI
contract. It shows the exact command, queues work off the UI thread, requires the selected
repository and campaign identity, and refuses an existing or in-repository destination before
running the coordinator. The coordinator now also rejects an in-repository destination, including
one reached through a linked parent, so direct command use preserves that boundary. The focused
runtime and direct CLI process regressions passed 1/1 each. The coordinator writes a private,
redacted directory and reports that nothing was uploaded. The UI checks that the manifest
accounts for exactly the five documented record names, with no duplicate or unexpected file.
A missing or mismatched receipt is treated as uncertain output: the UI does not retry
automatically and asks the owner to inspect the destination. The written test uses a disposable
repository and checks the manifest, unchanged
target and refusal paths. This action does not transmit or authorize sharing the bundle.

This is an increment under open Sub-task 36.3.2.4. The source notices are not the generated
licence inventory of a packaged release candidate. User-facing text is still embedded in Rust,
and a locale catalogue, 40% expansion and right-to-left checks and complete package About
inventory remain unimplemented. It is not a desktop, Orca, human, live-provider, package or
release qualification.

## Verification disposition

The focused offline Help and manual-diagnostic tests passed 1/1 each; strict workspace Clippy
passed; the software-rendered native UI all-target suite passed 98/98. Python unittest passed
41/42, with only CM-R01.6's eight source-bound input drifts. The exact staged diff was inspected.
The first exact-commit independent review returned FINDINGS as recorded below. No task or gate is
marked complete by this increment.

The independent review of the initial Help commit found that campaign reselection could let a
delayed support response satisfy a later request to the same campaign. A correction candidate
now cancels older selection generations and requires an exact support request ID before consuming
pending state. Focused native Help integration passed 2/2, including a replayed real prior bundle
receipt. The cumulative software-rendered native UI suite passed 99/99 and strict workspace
Clippy passed. Python unittest passed 41/42 with only CM-R01.6's eight source-bound input
drifts. Independent exact-commit re-review remains open; the original commit's local test
result is not a review PASS.
