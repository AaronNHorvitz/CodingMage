"""Integrity checks for the required multi-agent scenario matrix."""

from __future__ import annotations

import copy
import hashlib
import json
from pathlib import Path
import re
import subprocess
import unittest


ROOT = Path(__file__).resolve().parents[1]
MATRIX = ROOT / "docs" / "evidence" / "multi-agent-scenario-matrix.json"
BINDING = ROOT / "docs" / "evidence" / "multi-agent-evidence-binding.json"
ALLOWED_STATUSES = {"local_automated", "local_automated_with_boundary", "external_blocked"}
REQUIRED_FIELDS = {
    "task_id",
    "claim",
    "status",
    "implementation",
    "test_refs",
    "limitations",
}
REQUIRED_BINDING_FIELDS = {
    "schema_version",
    "evidence_id",
    "source_commit",
    "commands",
    "command_set_sha256",
    "inputs",
    "package",
    "package_claim_sha256",
    "platform",
    "platform_claim_sha256",
}
REQUIRED_INPUT_GROUPS = {"implementation", "test", "schema", "fixture", "package"}
SHA256_PATTERN = re.compile(r"^[0-9a-f]{64}$")
COMMIT_PATTERN = re.compile(r"^[0-9a-f]{40}$")


def canonical_sha256(value: object) -> str:
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def file_sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def binding_errors(binding: dict[str, object]) -> list[str]:
    errors: list[str] = []
    if set(binding) != REQUIRED_BINDING_FIELDS:
        errors.append("binding-fields")
    if binding.get("schema_version") != 1:
        errors.append("schema-version")
    if binding.get("evidence_id") != "codingmage.multi_agent.local.v1":
        errors.append("evidence-id")

    source_commit = binding.get("source_commit")
    if not isinstance(source_commit, str) or not COMMIT_PATTERN.fullmatch(source_commit):
        errors.append("source-commit")
    else:
        observed = subprocess.run(
            ["git", "cat-file", "-e", f"{source_commit}^{{commit}}"],
            cwd=ROOT,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        if observed.returncode != 0:
            errors.append("source-commit-missing")

    commands = binding.get("commands")
    if (
        not isinstance(commands, list)
        or not commands
        or any(not isinstance(command, str) or not command for command in commands)
        or binding.get("command_set_sha256") != canonical_sha256(commands)
    ):
        errors.append("commands")

    inputs = binding.get("inputs")
    groups: set[str] = set()
    paths: set[str] = set()
    if not isinstance(inputs, list) or not inputs:
        errors.append("inputs")
    else:
        for entry in inputs:
            if not isinstance(entry, dict) or set(entry) != {"group", "path", "sha256"}:
                errors.append("input-shape")
                continue
            group = entry.get("group")
            relative = entry.get("path")
            expected = entry.get("sha256")
            if not isinstance(group, str) or group not in REQUIRED_INPUT_GROUPS:
                errors.append("input-group")
                continue
            groups.add(group)
            if (
                not isinstance(relative, str)
                or relative in paths
                or Path(relative).is_absolute()
                or ".." in Path(relative).parts
            ):
                errors.append("input-path")
                continue
            paths.add(relative)
            target = ROOT / relative
            if (
                not isinstance(expected, str)
                or not SHA256_PATTERN.fullmatch(expected)
                or not target.is_file()
                or file_sha256(target) != expected
            ):
                errors.append(f"input-drift:{relative}")
        if groups != REQUIRED_INPUT_GROUPS:
            errors.append("input-groups")

    package = binding.get("package")
    if (
        not isinstance(package, dict)
        or set(package) != {"archive", "sha256", "source_commit"}
        or package.get("archive") != "codingmage-0.1.0-linux-x86_64.tar.gz"
        or package.get("source_commit") != source_commit
        or not isinstance(package.get("sha256"), str)
        or not SHA256_PATTERN.fullmatch(str(package.get("sha256")))
        or binding.get("package_claim_sha256") != canonical_sha256(package)
    ):
        errors.append("package")

    platform = binding.get("platform")
    if (
        platform != {"architecture": "x86_64", "operating_system": "linux"}
        or binding.get("platform_claim_sha256") != canonical_sha256(platform)
    ):
        errors.append("platform")
    return errors


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

    def test_multi_agent_evidence_binding_is_current(self) -> None:
        binding = json.loads(BINDING.read_text(encoding="utf-8"))
        self.assertEqual(binding_errors(binding), [])

    def test_evidence_binding_rejects_each_stale_claim_class(self) -> None:
        binding = json.loads(BINDING.read_text(encoding="utf-8"))
        mutations = {
            "implementation": lambda value: value["inputs"][0].update(sha256="0" * 64),
            "test-command": lambda value: value["commands"].append("changed command"),
            "schema": lambda value: next(
                item for item in value["inputs"] if item["group"] == "schema"
            ).update(sha256="0" * 64),
            "fixture": lambda value: next(
                item for item in value["inputs"] if item["group"] == "fixture"
            ).update(sha256="0" * 64),
            "package": lambda value: value["package"].update(sha256="0" * 64),
            "platform": lambda value: value["platform"].update(operating_system="changed"),
        }
        for name, mutate in mutations.items():
            with self.subTest(name=name):
                changed = copy.deepcopy(binding)
                mutate(changed)
                self.assertTrue(binding_errors(changed))


if __name__ == "__main__":
    unittest.main()
