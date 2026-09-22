# Sprint 36 Native UI Verification

- **Status:** Deterministic product verification on disposable repositories with fake providers;
  no real desktop, screen reader, clean installation, live provider or independent review claim
- **Renderer:** offscreen `wgpu` through `egui_kittest` on the Mesa `llvmpipe` software adapter
  (Vulkan backend, CPU device type). Every screenshot below was rendered this way inside the
  development sandbox, which has no display server or GPU. They are not compositor screenshots.
- **Test source:** `crates/codingmage-ui/tests/verification.rs`; set
  `CODINGMAGE_UI_EVIDENCE_DIR` to retain artifacts. The retained full set and manifests live in
  private runtime state; a curated subset is committed under
  [`sprint-36-screenshots`](sprint-36-screenshots/).

## Commands

```text
cargo build -p codingmage-cli --locked
CODINGMAGE_UI_EVIDENCE_DIR=<private dir> cargo test -p codingmage-ui --test verification --locked -- --test-threads=1
```

Result: 6 passed, 0 failed. Earlier attempts whose failures were test assumptions (a duplicate
label query and a stop-after-unit sent before any unit was admitted) are retained as private
logs; the interface behaved correctly in each.

## Setup-to-outcome workflow through the interface

`setup_to_outcome_workflow_through_the_interface` drives only the interface's own actions: guided
configuration written through the existing loader, the owner's authorization record, campaign
authoring bound to the live diagnosis, readiness, real `campaign-preflight`, admission with the
confirmed report digest, a detached `codingmage campaign` launch with fake providers, one accepted
unit, stop-after-unit, inspection of changes and records, and a report export outside the
repository.

| Measurement | Value |
| --- | --- |
| Units completed by the invocation | 1 |
| Stop reason | `stop_after_unit` |
| Active checkout task source unchanged | true |
| Exported report states delivery as withheld and omits the repository path | asserted |

Screenshots (1100x720, scale 1.0): `01-overview-no-repository`, `02-setup-configuration-form`,
`03-overview-opened`, `04-work-plan`, `05-setup-campaign-form`, `06-campaign-readiness`,
`07-campaign-preflight-ready`, `08-campaign-admitted`, `09-campaign-live`,
`10-campaign-stopped`, `11-work-plan-overlay`, `12-changes-and-reviews`, `13-reports-exported`.

## Failure and recovery

`recovery_after_a_killed_coordinator_and_resumed_durable_state` admits and starts a campaign
whose implementer sleeps, kills the fixture coordinator with `SIGKILL`, and observes the interface.

| Measurement | Value |
| --- | --- |
| Interface state after the kill | "exited without a terminal outcome" with no stable code |
| Owned provider process alive three seconds after the coordinator was killed | false (the process guard terminated it) |
| Durable status readable after the crash | yes |
| Start allowed again under the same admission | yes |
| Admission and crash observation after reopening the interface | restored |
| Resumed invocation outcome | `paused`, `stop_after_unit`, 1 newly accepted unit, blocker `codingmage.campaign.control.stop_after_unit` |

Screenshots: `20-campaign-after-crash`, `21-campaign-resumed-after-crash`.

`stale_and_malformed_private_state_is_visible_and_never_fatal` writes a malformed launch record,
an admission that is not an object and a ledger that is a string; reopening treats each as absent,
start is refused as not admitted, and the target repository is untouched.

## Keyboard navigation

`keyboard_navigation_reaches_controls_in_order` presses `Tab` twelve times from a fresh window and
records the focused node's accessible label each time:

```text
Overview > Work plan > Campaign > Changes and reviews > Reports > Setup > Configuration file > Open > Browse > Overview > Work plan > Campaign
```

`Ctrl+2` and `Ctrl+6` switch to the work plan and setup screens; typing a configuration path
into the labelled field and pressing `Enter` opens the repository; `F5` issues a refresh. Text
fields carry accessible labels through `labelled_by`; disabled source checkboxes expose the
disabled state.

## Window sizes and scale

`window_sizes_and_high_dpi_keep_navigation_and_content_reachable` renders the opened overview at
four configurations and asserts that every navigation button and the diagnosis section remain in
the accessibility tree.

| Artifact | Logical size | Scale | Pixels |
| --- | --- | --- | --- |
| `30-compact-720x480` | 720x480 (the minimum window size) | 1.0 | 720x480 |
| `31-default-1100x720` | 1100x720 | 1.0 | 1100x720 |
| `32-high-dpi-1100x720-at-2x` | 1100x720 | 2.0 | 2200x1440 |
| `33-large-1920x1080` | 1920x1080 | 1.0 | 1920x1080 |

## Resource use and reactive repaint

`resource_use_is_bounded_and_idle_repaint_is_reactive` measured the test process itself (which
hosts the interface logic without a window):

| Measurement | Value |
| --- | --- |
| Repaint requested by the interface while idle without a repository | false |
| Mean logic-only frame time across all screens | 3 ms |
| Resident set before the interface was created | 203920 KiB (test binary, fonts and wgpu already loaded) |
| Resident set after rendering every screen | 204012 KiB |

Real window memory and idle CPU on a desktop are human-only item H1.

## Retained artifact digests

SHA-256 prefixes of the retained offscreen screenshots (full digests are in the private manifests):

```text
262011b3ac2490df 01-overview-no-repository.png
fa4dc274791b0916 02-setup-configuration-form.png
9d9701451c2a58df 03-overview-opened.png
69a0c81f38a1f7c7 04-work-plan.png
e6983cd99ea2cb9f 05-setup-campaign-form.png
82515b03f6492396 06-campaign-readiness.png
0a24c4e018a9a239 07-campaign-preflight-ready.png
c90f6f66fe2c5d34 08-campaign-admitted.png
2bd95b961f5c2767 09-campaign-live.png
d5bdfec51a79d442 10-campaign-stopped.png
25f936fa76e5a278 11-work-plan-overlay.png
0396a0084af7921c 12-changes-and-reviews.png
2ab2eb97f08b3a4c 13-reports-exported.png
8908749bdf4a7792 20-campaign-after-crash.png
464131c670df19fa 21-campaign-resumed-after-crash.png
5f3dad17061a81b4 30-compact-720x480.png
b5931c8c721b3f84 31-default-1100x720.png
afc119aa03b4ed9b 32-high-dpi-1100x720-at-2x.png
574b09b1902742da 33-large-1920x1080.png
```

The screenshots show sandbox fixture paths under a temporary directory; no host paths,
credentials or private repository content appear in them.

## What this does not prove

Real Wayland or X11 windows, compositor decorations, screen-reader announcements, installation on
a clean desktop, live providers and independent review are open in
[the human-only register](sprint-36-human-only.md).
