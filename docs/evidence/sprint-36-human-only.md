# Sprint 36 Human-Only and External Items

- **Status:** Open register; nothing here is claimed. Each row names who must act, the exact
  steps, and what evidence closes it.
- **Source revision:** the commit recorded in the development handoff for the native UI branch

These items need a person, a real desktop session, an approved provider account or independent
review authority that the sandboxed implementation agent did not have. They keep Sub-task
36.2.3.2, AC 36.1, AC 36.2, Gate 36.1 and Gate 36.2 open.

## H1 - Real desktop launch on Wayland and X11

Who: the repository owner or a designated tester on Fedora and Ubuntu desktops.

Steps:

1. `cargo build --locked --release -p codingmage-cli -p codingmage-ui` from a clean checkout of
   the recorded commit.
2. Launch `target/release/codingmage-ui` from a Wayland session (GNOME on Fedora) and from an
   X11 session (`XDG_SESSION_TYPE=x11` or an Xorg login); confirm a window with client-side
   decorations appears, resizes to the 720x480 minimum, and remains usable at 200 percent
   scaling.
3. Confirm an idle window shows no CPU use in `top` after the first frame and after a campaign
   is selected (the interface polls status every fifteen seconds).
4. Close the window while a fixture coordinator is running and confirm the process survives.

Evidence: screenshots from the real compositor (not the offscreen renderer), the `top` reading,
and the exact commit. Record the compositor, GPU or driver, and scaling factor.

## H2 - Screen-reader verification with an assistive tool

Who: a tester with Orca (GNOME) or another AT-SPI screen reader.

Steps:

1. Start Orca, then launch `codingmage-ui`.
2. Navigate with `Tab`, `Shift+Tab`, `Ctrl+1` to `Ctrl+6` and arrow keys; confirm each
   navigation button, text field, checkbox, combo box and status message is announced with its
   label and state (the disabled source checkboxes must be announced as unavailable).
3. Open a fixture repository and confirm the failure box text and the readiness check results
   are reachable and announced.

Evidence: a written log of what was announced for each control, any control that was silent or
mislabelled, and the Orca version.

## H3 - Installation and launch on a clean desktop

Who: a tester with a fresh Fedora or Ubuntu user account.

Steps:

1. Install `codingmage` and `codingmage-ui` into one prefix (the existing installer covers only
   `codingmage`; place `codingmage-ui` next to it).
2. Launch `codingmage-ui` with no prior `~/.config/codingmage-ui` state; confirm the empty
   workspace appears, the coordinator is found, and a missing font or missing coordinator produces
   the documented failure instead of a crash.
3. Remove the prefix and confirm the private state directory is the only residue.

Evidence: package or prefix identity, the observed first screen, and the residue listing.

## H4 - Separately admitted real-provider run

Who: the repository owner, under the existing controlled-target authority.

Steps:

1. Author a campaign in Setup against a disposable repository with the real `claude` and `codex`
   executables and existing logins; publication local only.
2. Run preflight from the Campaign screen and inspect the report; admit only if every provider
   probe is verified.
3. Start the coordinator from the interface, observe one accepted unit, stop after unit, and
   inspect the changes, review record and report.

Evidence: the preflight report digest, the campaign identity, the terminal outcome, and the
provider versions. Quota, authentication or terms failures must be recorded as such; the
interface must show them as failures, not retry with a different model.

## H5 - Independent review of the interface

Who: an independent reviewer who did not author the interface.

Steps: review the exact commits recorded in the handoff for the boundaries in Decisions 0015 and
0016 (no authority in widgets, no coordinator adoption, no credential handling, strict contract
models, export safeguards) and the evidence in
[the local evidence record](sprint-36-native-ui-local.md) and
[the verification record](sprint-36-native-ui-verification.md).

Evidence: findings on the exact commit; corrections are implemented separately and re-reviewed.

## H6 - Package and release gates

Sprint 36 does not change the release scope. Packaging `codingmage-ui`, signing, publication and
the public-release checklist remain governed by Sprints 26 and 27 and External 7 and 8.
