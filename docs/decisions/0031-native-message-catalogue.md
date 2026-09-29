# Decision 0031: Bundled Native Message Catalogue

- **Status:** Accepted for incremental Sprint 36.3 implementation
- **Date:** 2026-09-29
- **Decision owners:** Repository owner (implementation choice delegated)
- **Supersedes:** None
- **Superseded by:** None

## Context

Decision 0020 requires externalized UI text, 40% expansion testing and right-to-left layout.
The current native UI has English text embedded throughout its Rust screens. Help and About
are available offline and provide a bounded first slice of this migration.

## Decision

1. Store authored English messages in a versioned TOML catalogue bundled into the native
   executable. Validate the schema, exact key set and nonempty values before rendering.
   Message keys stay stable while the catalogue grows; a missing or unknown key is a build/test
   defect, never silent fallback text.
2. Migrate Help orientation, keyboard guidance, glossary and source-notice labels first. The
   shipped app continues to display English while other screens and dynamic text remain
   embedded. No incomplete language selector or pseudo-locale appears in Settings.
3. Generate synthetic expanded and right-to-left-prefixed messages only in tests. Exercise
   the Help layout at the minimum window and 200% scale with right alignment. These tests are
   layout stress checks; they do not assert full bidirectional text, screen-reader or human
   qualification.
4. Use the already pinned `toml` and `serde` crates. This introduces no dependency, process,
   network request, coordinator command or authority path.

## Consequences

The catalogue becomes the source for the migrated Help text. The rest of the text inventory,
locale formatting, full right-to-left navigation, packaged third-party licence list and human
accessibility checks remain open under Task 36.3.2.4 and Story 36.3.3.

## Subsequent incremental use

The next increment adds the Help manual-diagnostics labels and Settings appearance labels to
the same bundled catalogue. The schema shape stays at version one; key additions ship in the
same binary as their callers and the exact-key validator rejects incomplete bundles. The
Settings preview exercises 40% longer and right-aligned synthetic text at the minimum window
and 200% scale. This does not add a language selector or qualify application-wide RTL.
