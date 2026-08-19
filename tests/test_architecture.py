from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "check_architecture.py"
SPEC = importlib.util.spec_from_file_location("check_architecture", SCRIPT)
assert SPEC and SPEC.loader
check_architecture = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = check_architecture
SPEC.loader.exec_module(check_architecture)


class ArchitecturePolicyTest(unittest.TestCase):
    def test_seeded_forbidden_edge_names_exact_dependency(self) -> None:
        metadata = {
            "packages": [
                {
                    "name": "codingmage-contracts",
                    "dependencies": [{"name": "codingmage-core"}],
                },
                {"name": "codingmage-core", "dependencies": []},
            ]
        }
        policy = {
            "allowed_internal_dependencies": {
                "codingmage-contracts": [],
                "codingmage-core": ["codingmage-contracts"],
            }
        }
        self.assertEqual(
            ["architecture.edge.forbidden: codingmage-contracts -> codingmage-core"],
            check_architecture.validate_edges(metadata, policy),
        )

    def test_allowed_direction_passes(self) -> None:
        metadata = {
            "packages": [
                {
                    "name": "codingmage-core",
                    "dependencies": [{"name": "codingmage-contracts"}],
                },
                {"name": "codingmage-contracts", "dependencies": [{"name": "serde"}]},
            ]
        }
        policy = {
            "allowed_internal_dependencies": {
                "codingmage-contracts": [],
                "codingmage-core": ["codingmage-contracts"],
            }
        }
        self.assertEqual([], check_architecture.validate_edges(metadata, policy))

    def test_explicit_test_only_edge_does_not_grant_production_edge(self) -> None:
        policy = {
            "allowed_internal_dependencies": {
                "codingmage-soak": [],
                "codingmage-orchestrator": [],
            },
            "allowed_internal_dev_dependencies": {
                "codingmage-soak": ["codingmage-orchestrator"],
            },
        }
        test_only = {
            "packages": [
                {
                    "name": "codingmage-soak",
                    "dependencies": [
                        {"name": "codingmage-orchestrator", "kind": "dev"}
                    ],
                },
                {"name": "codingmage-orchestrator", "dependencies": []},
            ]
        }
        production = {
            "packages": [
                {
                    "name": "codingmage-soak",
                    "dependencies": [
                        {"name": "codingmage-orchestrator", "kind": None}
                    ],
                },
                {"name": "codingmage-orchestrator", "dependencies": []},
            ]
        }
        self.assertEqual([], check_architecture.validate_edges(test_only, policy))
        self.assertEqual(
            ["architecture.edge.forbidden: codingmage-soak -> codingmage-orchestrator"],
            check_architecture.validate_edges(production, policy),
        )


if __name__ == "__main__":
    unittest.main()
