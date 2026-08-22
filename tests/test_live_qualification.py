"""Tests for the explicit live-qualification boundary."""

from __future__ import annotations

import importlib.util
import io
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "qualify_campaign", ROOT / "scripts" / "qualify_campaign.py"
)
assert SPEC is not None and SPEC.loader is not None
QUALIFY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(QUALIFY)


class LiveQualificationTests(unittest.TestCase):
    def campaign(self, publication: str, destination: str, github: bool) -> dict[str, object]:
        policy: dict[str, object] = {
            "publication_mode": publication,
            "destination_promotion_policy": destination,
        }
        if github:
            policy["github"] = {"owner": "fixture", "repository": "fixture"}
        return {
            "repository_path": "/tmp/qualification-fixture",
            "initial_commit": "a" * 40,
            "multi_agent": policy,
        }

    def test_provider_mode_requires_local_only_authority(self) -> None:
        repository, commit = QUALIFY.validate_campaign(
            self.campaign("local_only", "human_required", False), "providers"
        )
        self.assertEqual(repository, Path("/tmp/qualification-fixture"))
        self.assertEqual(commit, "a" * 40)
        with self.assertRaises(QUALIFY.QualificationError):
            QUALIFY.validate_campaign(
                self.campaign("per_task_draft_pull_request", "human_required", True),
                "providers",
            )

    def test_remote_mode_rejects_missing_github_or_automatic_destination(self) -> None:
        with self.assertRaises(QUALIFY.QualificationError):
            QUALIFY.validate_campaign(
                self.campaign("per_task_draft_pull_request", "human_required", False),
                "github",
            )
        with self.assertRaises(QUALIFY.QualificationError):
            QUALIFY.validate_campaign(
                self.campaign("per_task_draft_pull_request", "auto_to_default_branch", True),
                "full",
            )

    def test_command_sequence_has_one_preflight_before_one_campaign(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            commands = QUALIFY.qualification_commands(
                root / "codingmage", root / "config.toml", root / "campaign.toml"
            )
        self.assertEqual(commands[0][1], "doctor")
        self.assertEqual(commands[1][1], "campaign")
        self.assertEqual(len(commands), 2)

    def test_main_refuses_before_file_or_process_access_without_acknowledgment(self) -> None:
        arguments = [
            "--mode",
            "providers",
            "--binary",
            "/missing/codingmage",
            "--config",
            "/missing/config.toml",
            "--campaign",
            "/missing/campaign.toml",
        ]
        stderr = io.StringIO()
        with mock.patch.dict(os.environ, {}, clear=True), mock.patch("sys.stderr", stderr):
            self.assertEqual(QUALIFY.main(arguments), 2)
        self.assertIn("acknowledgment is absent", stderr.getvalue())


if __name__ == "__main__":
    unittest.main()
