#!/usr/bin/env python3
"""Run an explicitly authorized live campaign qualification."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import subprocess
import sys
import tomllib


ACKNOWLEDGMENT = "I_ACKNOWLEDGE_EXTERNAL_EFFECTS"
LOCAL_PUBLICATION = "local_only"
REMOTE_PUBLICATION = "per_task_draft_pull_request"
SAFE_DESTINATION_POLICIES = {"never", "human_required"}


class QualificationError(ValueError):
    """A live qualification precondition failed closed."""


def absolute_file(value: str, label: str, *, executable: bool = False) -> Path:
    """Return one validated absolute ordinary file."""
    path = Path(value)
    if not path.is_absolute() or not path.is_file() or path.is_symlink():
        raise QualificationError(f"invalid {label}")
    if executable and not os.access(path, os.X_OK):
        raise QualificationError(f"invalid {label}")
    return path


def load_campaign(path: Path) -> dict[str, object]:
    """Load one closed TOML campaign document."""
    try:
        document = tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, tomllib.TOMLDecodeError) as error:
        raise QualificationError("invalid campaign document") from error
    if not isinstance(document, dict):
        raise QualificationError("invalid campaign document")
    return document


def validate_campaign(document: dict[str, object], mode: str) -> tuple[Path, str]:
    """Validate publication and destination authority for a qualification mode."""
    repository = document.get("repository_path")
    initial_commit = document.get("initial_commit")
    policy = document.get("multi_agent")
    if (
        not isinstance(repository, str)
        or not Path(repository).is_absolute()
        or not isinstance(initial_commit, str)
        or len(initial_commit) not in {40, 64}
        or not isinstance(policy, dict)
    ):
        raise QualificationError("invalid campaign authority")
    publication = policy.get("publication_mode")
    destination = policy.get("destination_promotion_policy")
    expected_publication = LOCAL_PUBLICATION if mode == "providers" else REMOTE_PUBLICATION
    if publication != expected_publication or destination not in SAFE_DESTINATION_POLICIES:
        raise QualificationError("qualification mode does not match safe campaign policy")
    github = policy.get("github")
    if mode == "providers" and github is not None:
        raise QualificationError("provider qualification cannot carry remote authority")
    if mode != "providers" and not isinstance(github, dict):
        raise QualificationError("remote qualification requires exact GitHub authority")
    return Path(repository), initial_commit


def verify_repository(repository: Path, initial_commit: str) -> None:
    """Require the exact clean target commit before any provider or remote effect."""
    if not repository.is_dir() or repository.is_symlink():
        raise QualificationError("invalid target repository")
    status = subprocess.run(
        ["/usr/bin/git", "-C", str(repository), "status", "--porcelain=v1"],
        check=False,
        capture_output=True,
        text=False,
        timeout=30,
    )
    head = subprocess.run(
        ["/usr/bin/git", "-C", str(repository), "rev-parse", "HEAD"],
        check=False,
        capture_output=True,
        text=True,
        timeout=30,
    )
    if status.returncode != 0 or status.stdout or head.returncode != 0:
        raise QualificationError("target repository is not clean and observable")
    if head.stdout.strip() != initial_commit:
        raise QualificationError("target repository head differs from campaign authority")


def qualification_commands(binary: Path, config: Path, campaign: Path) -> list[list[str]]:
    """Build the fixed preflight and campaign command sequence."""
    return [
        [str(binary), "doctor", "--config", str(config)],
        [
            str(binary),
            "campaign",
            "--config",
            str(config),
            "--campaign",
            str(campaign),
        ],
    ]


def parse_args(arguments: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", choices=("providers", "github", "full"), required=True)
    parser.add_argument("--binary", required=True)
    parser.add_argument("--config", required=True)
    parser.add_argument("--campaign", required=True)
    parser.add_argument("--max-runtime-seconds", type=int, default=21_600)
    return parser.parse_args(arguments)


def main(arguments: list[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if arguments is None else arguments)
    try:
        if os.environ.get("CODINGMAGE_LIVE_QUALIFICATION") != ACKNOWLEDGMENT:
            raise QualificationError("live qualification acknowledgment is absent")
        if not 60 <= args.max_runtime_seconds <= 86_400:
            raise QualificationError("invalid qualification runtime ceiling")
        binary = absolute_file(args.binary, "CodingMage executable", executable=True)
        config = absolute_file(args.config, "configuration")
        campaign = absolute_file(args.campaign, "campaign specification")
        document = load_campaign(campaign)
        repository, initial_commit = validate_campaign(document, args.mode)
        verify_repository(repository, initial_commit)
        for command in qualification_commands(binary, config, campaign):
            completed = subprocess.run(command, check=False, timeout=args.max_runtime_seconds)
            if completed.returncode != 0:
                raise QualificationError("live qualification command failed")
    except (QualificationError, OSError, subprocess.SubprocessError) as error:
        print(f"qualification refused: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
