# Third-Party Notices

## Source Dependencies

CodingMage is licensed under Apache-2.0. It also depends on third-party Rust crates and external
operator-installed tools whose licenses remain their respective owners' responsibility.

The release packager generates an SPDX 2.3 software bill of materials from the exact locked Cargo
dependency graph. That generated SBOM, the corresponding `Cargo.lock`, and the packaged build
manifest are the authoritative dependency inventory for a candidate. A source checkout alone is
not evidence that a final release artifact contains the same graph.

## Interoperable Tools

CodingMage can invoke separately installed Git, Claude Code, Codex, Cargo, Node.js tooling, and
GitHub CLI surfaces when an operator explicitly configures them. Those products are not bundled by
CodingMage. Their names identify interoperability only and do not imply sponsorship, endorsement,
redistribution rights, or a license grant from their owners.

## Redistribution Check

Before publication, the candidate must include its SPDX SBOM, locked dependency inventory, this
notice, and every license file required by the exact bundled contents. Any unresolved,
unidentified, or incompatible dependency license blocks release publication.
