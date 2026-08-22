"""Integrity checks for the required multi-agent scenario matrix."""

from __future__ import annotations

import json
from pathlib import Path
import re
import subprocess
import unittest


ROOT = Path(__file__).resolve().parents[1]
MATRIX = ROOT / "docs" / "evidence" / "multi-agent-scenario-matrix.json"
ALLOWED_STATUSES = {"local_automated", "local_automated_with_boundary", "external_blocked"}
REQUIRED_FIELDS = {
    "task_id",
    "claim",
    "status",
    "implementation",
    "test_refs",
    "limitations",
}


class MultiAgentScenarioMatrixTests(unittest.TestCase):
    def load_matrix(self) -> dict[str, object]:
        return json.loads(MATRIX.read_text(encoding="utf-8"))

    def test_matrix_has_every_required_scenario_once(self) -> None:
        matrix = self.load_matrix()
        self.assertEqual(matrix["schema_version"], 1)
        scenarios = matrix["scenarios"]
        self.assertIsInstance(scenarios, list)
        expected = [f"25.2.5.{number}" for number in range(1, 45)]
        self.assertEqual([scenario["task_id"] for scenario in scenarios], expected)

    def test_every_scenario_binds_existing_implementation_and_test(self) -> None:
        for scenario in self.load_matrix()["scenarios"]:
            self.assertEqual(set(scenario), REQUIRED_FIELDS)
            self.assertIn(scenario["status"], ALLOWED_STATUSES)
            self.assertTrue(scenario["claim"])
            self.assertTrue(scenario["limitations"])
            self.assertTrue(scenario["implementation"])
            self.assertTrue(scenario["test_refs"])

            for relative in scenario["implementation"]:
                self.assertTrue((ROOT / relative).is_file(), relative)

            for reference in scenario["test_refs"]:
                relative, separator, symbol = reference.partition("::")
                self.assertEqual(separator, "::", reference)
                source = ROOT / relative
                self.assertTrue(source.is_file(), relative)
                declaration = re.compile(rf"\b(?:fn|def)\s+{re.escape(symbol)}\(")
                self.assertRegex(source.read_text(encoding="utf-8"), declaration, reference)

    def test_private_downstream_name_is_absent_from_tracked_content(self) -> None:
        prohibited = "".join(("agent", "mage"))
        tracked = subprocess.run(
            ["git", "grep", "-ni", prohibited, "--", ".", ":!target"],
            cwd=ROOT,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        self.assertEqual(tracked.returncode, 1, tracked.stdout)
        self.assertEqual(tracked.stdout, "")
        self.assertEqual(tracked.stderr, "")


if __name__ == "__main__":
    unittest.main()
