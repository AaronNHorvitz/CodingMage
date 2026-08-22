#!/usr/bin/env python3
"""Rootless install, verify, rollback, and removal for a CodingMage archive."""

from __future__ import annotations

import argparse
import hashlib
import os
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
from pathlib import Path


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def paths(prefix: Path) -> tuple[Path, Path, Path]:
    binary = prefix / "bin" / "codingmage"
    return binary, binary.with_suffix(".previous"), prefix / "share" / "codingmage" / "install.sha256"


def service_paths(prefix: Path, unit_root: Path) -> tuple[Path, Path]:
    return unit_root / "codingmage.service", prefix / "share" / "codingmage" / "service.sha256"


def ordinary_file(path: Path, label: str) -> Path:
    if not path.is_absolute() or not path.is_file() or path.is_symlink():
        raise ValueError(f"invalid {label}")
    return path.resolve(strict=True)


def ordinary_directory(path: Path, label: str) -> Path:
    if not path.is_absolute() or not path.is_dir() or path.is_symlink():
        raise ValueError(f"invalid {label}")
    return path.resolve(strict=True)


def systemd_argument(path: Path) -> str:
    value = str(path)
    if any(character in value for character in ("\n", "\r", "\0")):
        raise ValueError("invalid systemd path")
    escaped = value.replace("%", "%%").replace("\\", "\\\\").replace('"', '\\"')
    return f'"{escaped}"'


def runtime_roots(configuration: Path) -> tuple[Path, Path]:
    try:
        document = tomllib.loads(configuration.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, tomllib.TOMLDecodeError) as error:
        raise ValueError("invalid configuration") from error
    state = document.get("state_root")
    scratch = document.get("scratch_root")
    if not isinstance(state, str) or not isinstance(scratch, str):
        raise ValueError("configuration roots missing")
    return ordinary_directory(Path(state), "state root"), ordinary_directory(
        Path(scratch), "scratch root"
    )


def render_service(prefix: Path, configuration: Path, campaign: Path) -> bytes:
    binary, _, _ = paths(prefix)
    binary = ordinary_file(binary, "installed binary")
    configuration = ordinary_file(configuration, "configuration")
    campaign = ordinary_file(campaign, "campaign")
    state, scratch = runtime_roots(configuration)
    return (
        "[Unit]\n"
        "Description=CodingMage local development coordinator\n"
        "After=default.target\n\n"
        "[Service]\n"
        "Type=simple\n"
        f"ExecStart={systemd_argument(binary)} campaign --config "
        f"{systemd_argument(configuration)} --campaign {systemd_argument(campaign)}\n"
        "Restart=on-failure\n"
        "RestartSec=10s\n"
        "KillMode=control-group\n"
        "TimeoutStopSec=30s\n"
        "NoNewPrivileges=true\n"
        "PrivateTmp=true\n"
        "ProtectSystem=strict\n"
        "ProtectHome=read-only\n"
        f"ReadWritePaths={systemd_argument(state)} {systemd_argument(scratch)}\n"
        "MemoryMax=4294967296\n"
        "TasksMax=256\n"
        "CPUQuota=400%\n\n"
        "[Install]\n"
        "WantedBy=default.target\n"
    ).encode("utf-8")


def run_systemctl(*arguments: str) -> None:
    subprocess.run(
        ["/usr/bin/systemctl", "--user", *arguments],
        check=True,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )


def install_service(
    prefix: Path,
    unit_root: Path,
    configuration: Path,
    campaign: Path,
    controller=run_systemctl,
) -> None:
    unit, receipt = service_paths(prefix, unit_root)
    content = render_service(prefix, configuration, campaign)
    unit_root.mkdir(parents=True, exist_ok=True)
    ordinary_directory(unit_root, "unit root")
    if unit.exists():
        if unit.is_symlink() or not unit.is_file() or unit.read_bytes() != content:
            raise ValueError("refusing changed service unit")
    else:
        descriptor = os.open(unit, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(content)
            stream.flush()
            os.fsync(stream.fileno())
    receipt.parent.mkdir(parents=True, exist_ok=True)
    receipt.write_text(hashlib.sha256(content).hexdigest() + "\n", encoding="ascii")
    controller("daemon-reload")


def verify_service(prefix: Path, unit_root: Path) -> None:
    unit, receipt = service_paths(prefix, unit_root)
    if (
        not unit.is_file()
        or unit.is_symlink()
        or not receipt.is_file()
        or receipt.is_symlink()
        or sha256(unit) != receipt.read_text(encoding="ascii").strip()
    ):
        raise ValueError("service installation changed")


def control_service(prefix: Path, unit_root: Path, action: str, controller=run_systemctl) -> None:
    if action not in {"start", "stop"}:
        raise ValueError("invalid service action")
    verify_service(prefix, unit_root)
    controller(action, "codingmage.service")


def remove_service(prefix: Path, unit_root: Path, controller=run_systemctl) -> None:
    unit, receipt = service_paths(prefix, unit_root)
    if not unit.exists() and not receipt.exists():
        return
    verify_service(prefix, unit_root)
    controller("stop", "codingmage.service")
    unit.unlink()
    receipt.unlink()
    controller("daemon-reload")


def safe_extract(archive: Path, destination: Path) -> Path:
    with tarfile.open(archive, "r:gz") as bundle:
        members = bundle.getmembers()
        if not members:
            raise ValueError("empty archive")
        for member in members:
            target = (destination / member.name).resolve()
            if not target.is_relative_to(destination.resolve()) or member.issym() or member.islnk():
                raise ValueError("unsafe archive member")
        bundle.extractall(destination, members=members, filter="data")
    roots = [path for path in destination.iterdir() if path.is_dir()]
    if len(roots) != 1:
        raise ValueError("invalid archive layout")
    return roots[0]


def install(archive: Path, prefix: Path) -> None:
    binary, previous, receipt = paths(prefix)
    with tempfile.TemporaryDirectory(prefix="codingmage-install-") as temporary:
        root = safe_extract(archive, Path(temporary))
        source = root / "bin" / "codingmage"
        if not source.is_file():
            raise ValueError("binary missing")
        expected = None
        for line in (root / "SHA256SUMS").read_text(encoding="ascii").splitlines():
            digest, name = line.split("  ", 1)
            candidate = root / name
            if not candidate.is_file() or sha256(candidate) != digest:
                raise ValueError("checksum mismatch")
            if name == "bin/codingmage":
                expected = digest
        if expected is None:
            raise ValueError("binary checksum missing")
        binary.parent.mkdir(parents=True, exist_ok=True)
        receipt.parent.mkdir(parents=True, exist_ok=True)
        if binary.exists():
            os.replace(binary, previous)
        staged = binary.with_suffix(".new")
        shutil.copy2(source, staged)
        staged.chmod(0o755)
        os.replace(staged, binary)
        receipt.write_text(expected + "\n", encoding="ascii")


def verify(prefix: Path) -> None:
    binary, _, receipt = paths(prefix)
    if not binary.is_file() or not receipt.is_file():
        raise ValueError("installation missing")
    if sha256(binary) != receipt.read_text(encoding="ascii").strip():
        raise ValueError("installation changed")


def rollback(prefix: Path) -> None:
    binary, previous, receipt = paths(prefix)
    if not binary.is_file() or not previous.is_file():
        raise ValueError("rollback unavailable")
    current = binary.with_suffix(".rollback")
    os.replace(binary, current)
    os.replace(previous, binary)
    os.replace(current, previous)
    receipt.parent.mkdir(parents=True, exist_ok=True)
    receipt.write_text(sha256(binary) + "\n", encoding="ascii")


def remove(prefix: Path, purge_data: bool) -> None:
    binary, previous, receipt = paths(prefix)
    default_unit, service_receipt = service_paths(
        prefix, Path.home() / ".config" / "systemd" / "user"
    )
    if default_unit.exists() or service_receipt.exists():
        raise ValueError("remove the CodingMage user service first")
    for path in [binary, previous, receipt]:
        if path.exists():
            if path.is_symlink() or not path.is_file():
                raise ValueError("refusing changed installation path")
            path.unlink()
    if purge_data:
        state = Path.home() / ".local" / "share" / "codingmage"
        config = Path.home() / ".config" / "codingmage"
        for path in [state, config]:
            if path.exists():
                if path.is_symlink() or not path.is_dir():
                    raise ValueError("refusing changed data path")
                shutil.rmtree(path)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "action",
        choices=[
            "install",
            "verify",
            "rollback",
            "remove",
            "service-install",
            "service-verify",
            "service-start",
            "service-stop",
            "service-remove",
        ],
    )
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--prefix", type=Path, default=Path.home() / ".local")
    parser.add_argument("--config", type=Path)
    parser.add_argument("--campaign", type=Path)
    parser.add_argument(
        "--unit-root", type=Path, default=Path.home() / ".config" / "systemd" / "user"
    )
    parser.add_argument("--purge-data", action="store_true")
    args = parser.parse_args()
    prefix = args.prefix.resolve()
    unit_root = args.unit_root.resolve()
    if args.action == "install":
        if args.archive is None:
            parser.error("install requires --archive")
        install(args.archive.resolve(), prefix)
    elif args.action == "verify":
        verify(prefix)
    elif args.action == "rollback":
        rollback(prefix)
    elif args.action == "remove":
        remove(prefix, args.purge_data)
    elif args.action == "service-install":
        if args.config is None or args.campaign is None:
            parser.error("service-install requires --config and --campaign")
        install_service(prefix, unit_root, args.config.resolve(), args.campaign.resolve())
    elif args.action == "service-verify":
        verify_service(prefix, unit_root)
    elif args.action == "service-start":
        control_service(prefix, unit_root, "start")
    elif args.action == "service-stop":
        control_service(prefix, unit_root, "stop")
    else:
        remove_service(prefix, unit_root)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
