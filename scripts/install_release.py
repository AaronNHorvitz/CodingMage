#!/usr/bin/env python3
"""Rootless install, verify, rollback, and removal for a CodingMage archive."""

from __future__ import annotations

import argparse
import hashlib
import os
import shutil
import tarfile
import tempfile
from pathlib import Path


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def paths(prefix: Path) -> tuple[Path, Path, Path]:
    binary = prefix / "bin" / "codingmage"
    return binary, binary.with_suffix(".previous"), prefix / "share" / "codingmage" / "install.sha256"


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
    parser.add_argument("action", choices=["install", "verify", "rollback", "remove"])
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--prefix", type=Path, default=Path.home() / ".local")
    parser.add_argument("--purge-data", action="store_true")
    args = parser.parse_args()
    prefix = args.prefix.resolve()
    if args.action == "install":
        if args.archive is None:
            parser.error("install requires --archive")
        install(args.archive.resolve(), prefix)
    elif args.action == "verify":
        verify(prefix)
    elif args.action == "rollback":
        rollback(prefix)
    else:
        remove(prefix, args.purge_data)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
