# Decision 0022: Inert Native Content and External Link Confirmation

- **Status:** Accepted for local native UI implementation
- **Date:** 2026-09-27
- **Decision owners:** CodingMage implementation worker under the owner's delegated UI scope
- **Supersedes:** None
- **Superseded by:** None

## Context

The native shell displays task titles, Git subjects and file names, durable review and gate
identities, coordinator activity, configured model names and report JSON. These strings can come
from a repository, a provider or a durable record influenced by either. Decision 0020 requires
them to be inert and permits an external link only after the owner sees its full destination and
confirms it. Large fields must not turn a UI frame into an unbounded text layout.

## Decision

The shell renders untrusted strings as egui text. It does not parse Markdown or HTML and does not
load images, scripts or remote resources from content. A shared content presenter replaces
formatting controls on the task detail, model/activity, changes/review and report-preview paths.
It makes Unicode direction controls and terminal control characters visible as replacement
characters and bounds one displayed field to 4,096 characters. Shortening is labelled; the full
report remains available through the existing guarded export action.

Only a complete ASCII HTTPS address with a simple DNS host is offered as a link candidate.
User-info, local host names, numeric addresses, malformed hosts, control characters and addresses
over 2,048 bytes are refused as clickable links. Every candidate is shown in full next to a
separate **Review external link** control. That control opens trusted shell confirmation; only a
second explicit **Open in browser** action asks the desktop to open the address. The address is
validated again at confirmation. No source string can create a campaign button, grant authority
or bypass this confirmation.

This adds no dependency or licence. It does not make an external destination trustworthy and
does not authorize network access in a headless test. Tests never select **Open in browser**.

## Consequences and verification

- The on-screen report document is labelled a preview when it exceeds the field bound; guarded
  export retains the full document.
- Plain egui labels for other bounded status values remain inert text. The UI has no content
  Markdown parser, image loader or direct hyperlink control.
- Unit fixtures cover script tags, remote image syntax, forged control text, direction and terminal
  control characters, deceptive link forms and oversized content. An offscreen interaction test
  checks that an external-link dialog appears only after a separate review click and closes on
  cancel. These are local software tests, not a browser or desktop security qualification.
- The CLI authority boundary and the complete per-screen state and command catalogue remain
  separate work under Sub-task 36.3.2.2.
