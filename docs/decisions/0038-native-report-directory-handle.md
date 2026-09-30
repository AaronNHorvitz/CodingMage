# ADR 0038: Retain the report export parent while publishing

- **Status:** Accepted
- **Date:** 2026-09-30
- **Decision owners:** CodingMage implementation

## Context

Decision 0037 rejects a destination whose parent already resolves inside the target
repository. The validation and subsequent pathname write still use separate directory
lookups. A local process can replace or move the named parent between those operations.
The existing export also checks absence before renaming a temporary file, so a concurrent
creator could be overwritten without the owner's overwrite choice.

## Decision

For the outcome report only, open the resolved existing parent as a Linux directory
descriptor without following its final link. Compare its device and inode to the parent
requested by the owner and check the descriptor's current path against the target before
staging, before publication and after publication. Stage a private file through that
descriptor, then publish without overwrite by an atomic hard link that fails when the
leaf already exists. With explicit overwrite, reject a non-regular existing leaf and
rename the staged file through the same directory descriptor. Check the published file's
identity against the staged file and remove it on an observable final mismatch. Refuse an
export when the selected target repository's device and inode have changed.
Validate once before formatting the report and again when opening the retained writer
handle, so a refused destination does not spend time serializing a large report and a
change during serialization is still checked.

The UI crate now directly declares the already pinned workspace `nix` 0.31.3 dependency
to open the directory with `O_DIRECTORY`, `O_NOFOLLOW` and `O_CLOEXEC`. This dependency is
already used by first-party crates and is MIT licensed. The rest of the writer uses the
standard Rust filesystem API through the retained Linux descriptor path. No provider,
credential, campaign or coordinator authority changes.

## Limits

This refuses deterministic alias and directory moves caught at each identity check and
prevents silent replacement by a concurrent creator when overwrite is false. A malicious
same-user process can still rename a directory between a check and the next filesystem
operation; checking afterwards cannot prove that no transient file reached the target.
Explicit overwrite still replaces a regular leaf by name after its type check; an
adversarial replacement of that leaf between operations cannot be ruled out. The UI
still performs this file I/O on the presentation thread and has no exact `codingmage`
command equivalent. Those section-2 and section-7 requirements remain open under Task
36.3.2.2, and stronger concurrent-adversary guarantees require a separate design and
verification boundary.

Decision 0045 adds a mount-ID boundary after an independent review found that a
bind-mounted repository alias was not covered by these path checks.
