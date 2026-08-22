"""Tests for the explicit live-qualification boundary."""

from __future__ import annotations

import importlib.util
import hashlib
import io
import json
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
    def qualification_fixture(self, root: Path) -> tuple[list[str], bytes]:
        target = root / "target"
        target.mkdir()
        binary = root / "codingmage"
        binary.write_text("#!/bin/sh\n", encoding="utf-8")
        binary.chmod(0o700)
        config = root / "config.toml"
        config.write_text("version = 1\n", encoding="utf-8")
        campaign = root / "campaign.toml"
        campaign.write_text(
            '\n'.join(
                (
                    f'repository_path = "{target}"',
                    f'initial_commit = "{"a" * 40}"',
                    "max_parallel_pods = 1",
                    "max_units = 10",
                    'publication = "local_only"',
                )
            )
            + "\n",
            encoding="utf-8",
        )
        authorization = root / "authorization.txt"
        authorization.write_text("approved fixture\n", encoding="utf-8")
        report = root / "preflight.json"
        arguments = [
            "--mode",
            "providers",
            "--binary",
            str(binary),
            "--config",
            str(config),
            "--campaign",
            str(campaign),
            "--authorization",
            str(authorization),
            "--preflight-report",
            str(report),
        ]
        preflight = json.dumps(
            {"schema_version": 2, "state": "ready", "source_free": True},
            sort_keys=True,
        ).encode("utf-8")
        return arguments, preflight

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
            "max_parallel_pods": 1,
            "max_units": 10,
            "publication": publication,
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
                root / "codingmage",
                root / "config.toml",
                root / "campaign.toml",
                root / "authorization.txt",
            )
        self.assertEqual(commands[0][1], "doctor")
        self.assertEqual(commands[1][1], "campaign-preflight")
        self.assertEqual(commands[2][1], "campaign")
        self.assertEqual(len(commands), 3)

    def test_main_refuses_missing_preflight_inputs_without_process_access(self) -> None:
        arguments = [
            "--mode",
            "providers",
            "--binary",
            "/missing/codingmage",
            "--config",
            "/missing/config.toml",
            "--campaign",
            "/missing/campaign.toml",
            "--authorization",
            "/missing/authorization.txt",
            "--preflight-report",
            "/missing/preflight.json",
        ]
        stderr = io.StringIO()
        with (
            mock.patch.dict(os.environ, {}, clear=True),
            mock.patch("sys.stderr", stderr),
            mock.patch("subprocess.run") as run,
        ):
            self.assertEqual(QUALIFY.main(arguments), 2)
        run.assert_not_called()
        self.assertIn("invalid CodingMage executable", stderr.getvalue())

    def test_main_stops_after_writing_unapproved_preflight_report(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            arguments, preflight = self.qualification_fixture(root)
            completed = (
                subprocess_result(0),
                subprocess_result(0, stdout=preflight),
            )
            stdout = io.StringIO()
            with (
                mock.patch.dict(os.environ, {}, clear=True),
                mock.patch.object(QUALIFY, "verify_repository"),
                mock.patch("subprocess.run", side_effect=completed) as run,
                mock.patch("sys.stdout", stdout),
            ):
                self.assertEqual(QUALIFY.main(arguments), 0)

            report = root / "preflight.json"
            self.assertEqual(report.read_bytes(), preflight)
            self.assertEqual(report.stat().st_mode & 0o777, 0o600)
            self.assertEqual(run.call_count, 2)
            digest = hashlib.sha256(preflight).hexdigest()
            self.assertIn(f"sha256={digest}", stdout.getvalue())

    def test_main_runs_campaign_only_after_exact_digest_and_acknowledgment(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            arguments, preflight = self.qualification_fixture(root)
            digest = hashlib.sha256(preflight).hexdigest()
            arguments.extend(("--approved-preflight-sha256", digest))
            completed = (
                subprocess_result(0),
                subprocess_result(0, stdout=preflight),
                subprocess_result(0),
            )
            with (
                mock.patch.dict(
                    os.environ,
                    {"CODINGMAGE_LIVE_QUALIFICATION": QUALIFY.ACKNOWLEDGMENT},
                    clear=True,
                ),
                mock.patch.object(QUALIFY, "verify_repository"),
                mock.patch("subprocess.run", side_effect=completed) as run,
            ):
                self.assertEqual(QUALIFY.main(arguments), 0)

            self.assertEqual(run.call_count, 3)
            self.assertEqual(run.call_args_list[-1].args[0][1], "campaign")

    def test_main_refuses_campaign_without_both_approval_conditions(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            arguments, preflight = self.qualification_fixture(root)
            digest = hashlib.sha256(preflight).hexdigest()
            cases = (
                ("0" * 64, QUALIFY.ACKNOWLEDGMENT, "digest does not match"),
                (digest, None, "acknowledgment is absent"),
            )
            for approved_digest, acknowledgment, message in cases:
                with self.subTest(message=message):
                    report = root / "preflight.json"
                    report.unlink(missing_ok=True)
                    invocation = [
                        *arguments,
                        "--approved-preflight-sha256",
                        approved_digest,
                    ]
                    environment = (
                        {}
                        if acknowledgment is None
                        else {"CODINGMAGE_LIVE_QUALIFICATION": acknowledgment}
                    )
                    stderr = io.StringIO()
                    completed = (
                        subprocess_result(0),
                        subprocess_result(0, stdout=preflight),
                    )
                    with (
                        mock.patch.dict(os.environ, environment, clear=True),
                        mock.patch.object(QUALIFY, "verify_repository"),
                        mock.patch("subprocess.run", side_effect=completed) as run,
                        mock.patch("sys.stderr", stderr),
                    ):
                        self.assertEqual(QUALIFY.main(invocation), 2)
                    self.assertEqual(run.call_count, 2)
                    self.assertIn(message, stderr.getvalue())

    def test_main_refuses_report_path_inside_target_before_process_access(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            arguments, _ = self.qualification_fixture(root)
            report_index = arguments.index("--preflight-report") + 1
            arguments[report_index] = str(root / "target" / "preflight.json")
            stderr = io.StringIO()
            with (
                mock.patch.dict(os.environ, {}, clear=True),
                mock.patch.object(QUALIFY, "verify_repository"),
                mock.patch("subprocess.run") as run,
                mock.patch("sys.stderr", stderr),
            ):
                self.assertEqual(QUALIFY.main(arguments), 2)
            run.assert_not_called()
            self.assertIn("cannot be stored in the target repository", stderr.getvalue())


def subprocess_result(returncode: int, *, stdout: bytes = b"") -> object:
    return type("Completed", (), {"returncode": returncode, "stdout": stdout})()


if __name__ == "__main__":
    unittest.main()
