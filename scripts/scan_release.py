#!/usr/bin/env python3
"""Inspect a CodingMage release candidate without extracting or executing it."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import tarfile
from pathlib import Path, PurePosixPath


VERSION = "0.1.0"
BINARY_ROOT = f"codingmage-{VERSION}"
SOURCE_ROOT = f"codingmage-{VERSION}-source"
BINARY_FILES = {
    "BUILD-MANIFEST.json",
    "DEPENDENCIES.json",
    "PROVENANCE.json",
    "SBOM.spdx.json",
    "SHA256SUMS",
    "bin/codingmage",
    "share/doc/codingmage/LICENSE",
    "share/doc/codingmage/README.md",
    "share/doc/codingmage/RELEASE-NOTES.md",
    "share/doc/codingmage/SECURITY.md",
    "share/doc/codingmage/SUPPORT.md",
    "share/doc/codingmage/THIRD-PARTY-NOTICES.md",
}
PROHIBITED_PARTS = {".env", ".git", "logs", "runtime-state", "scratch", "target"}
SECRET_PATTERNS = (
    re.compile(b"-----BEGIN " + b"PRIVATE KEY-----"),
    re.compile(rb"AKIA[0-9A-Z]{16}"),
    re.compile(rb"gh[pousr]_[A-Za-z0-9]{20,}"),
    re.compile(rb"xox[baprs]-[A-Za-z0-9-]{20,}"),
    re.compile(rb"AIza[0-9A-Za-z_-]{30,}"),
    re.compile(rb"sk-[A-Za-z0-9]{20,}"),
)


def sha256_bytes(content: bytes) -> str:
    return hashlib.sha256(content).hexdigest()


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def ordinary_file(path: Path, label: str) -> Path:
    if not path.is_absolute() or not path.is_file() or path.is_symlink():
        raise ValueError(f"invalid {label}")
    return path.resolve(strict=True)


def ordinary_directory(path: Path, label: str) -> Path:
    if not path.is_absolute() or not path.is_dir() or path.is_symlink():
        raise ValueError(f"invalid {label}")
    return path.resolve(strict=True)


def git(source: Path, *arguments: str) -> bytes:
    return subprocess.run(
        ["/usr/bin/git", "-C", str(source), *arguments],
        check=True,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    ).stdout


def tracked_files(source: Path) -> set[str]:
    if git(source, "status", "--porcelain=v1", "--untracked-files=all"):
        raise ValueError("release source is dirty")
    files = {
        item.decode("utf-8")
        for item in git(source, "ls-files", "-z").split(b"\0")
        if item
    }
    if not files:
        raise ValueError("release source inventory is empty")
    for name in files:
        path = source / name
        if not path.is_file() or path.is_symlink():
            raise ValueError("release source contains unsupported tracked entry")
    return files


def archive_files(archive: Path, expected_root: str) -> tuple[dict[str, bytes], set[str]]:
    files: dict[str, bytes] = {}
    executable: set[str] = set()
    with tarfile.open(archive, "r:gz") as bundle:
        members = bundle.getmembers()
        if not members:
            raise ValueError("release archive is empty")
        for member in members:
            name = PurePosixPath(member.name)
            if (
                name.is_absolute()
                or ".." in name.parts
                or not name.parts
                or name.parts[0] != expected_root
                or member.issym()
                or member.islnk()
                or not (member.isfile() or member.isdir())
            ):
                raise ValueError("release archive contains unsafe entry")
            relative = PurePosixPath(*name.parts[1:]).as_posix()
            if any(part in PROHIBITED_PARTS for part in name.parts[1:]):
                raise ValueError("release archive contains runtime or build residue")
            if member.isfile():
                if not relative or relative in files:
                    raise ValueError("release archive contains duplicate file")
                stream = bundle.extractfile(member)
                if stream is None:
                    raise ValueError("release archive file is unreadable")
                files[relative] = stream.read()
                if member.mode & 0o111:
                    executable.add(relative)
    return files, executable


def scan_sensitive(content: bytes, private_markers: set[bytes]) -> None:
    if any(pattern.search(content) for pattern in SECRET_PATTERNS):
        raise ValueError("release content contains a credential signature")
    if any(marker and marker in content for marker in private_markers):
        raise ValueError("release content contains a private local path")


def parse_checksums(content: bytes) -> dict[str, str]:
    try:
        lines = content.decode("ascii").splitlines()
    except UnicodeDecodeError as error:
        raise ValueError("package checksum inventory is invalid") from error
    checksums: dict[str, str] = {}
    for line in lines:
        parts = line.split("  ", 1)
        if (
            len(parts) != 2
            or not re.fullmatch(r"[0-9a-f]{64}", parts[0])
            or not parts[1]
            or parts[1] in checksums
        ):
            raise ValueError("package checksum inventory is invalid")
        checksums[parts[1]] = parts[0]
    return checksums


def scan(source: Path, binary_archive: Path, source_archive: Path) -> dict[str, object]:
    source = ordinary_directory(source, "source root")
    binary_archive = ordinary_file(binary_archive, "binary archive")
    source_archive = ordinary_file(source_archive, "source archive")
    expected_source = tracked_files(source)
    source_files, source_executables = archive_files(source_archive, SOURCE_ROOT)
    if set(source_files) != expected_source:
        raise ValueError("source archive does not match tracked source inventory")

    binary_files, binary_executables = archive_files(binary_archive, BINARY_ROOT)
    if set(binary_files) != BINARY_FILES:
        raise ValueError("binary archive contains undeclared or missing files")
    if binary_executables != {"bin/codingmage"}:
        raise ValueError("binary archive executable inventory is invalid")

    checksums = parse_checksums(binary_files["SHA256SUMS"])
    expected_checksums = set(binary_files) - {"SHA256SUMS"}
    if set(checksums) != expected_checksums:
        raise ValueError("package checksum inventory does not match package files")
    for name, digest in checksums.items():
        if sha256_bytes(binary_files[name]) != digest:
            raise ValueError("package checksum does not match package content")

    private_markers = {
        str(source).encode(),
        str(Path.home().resolve()).encode(),
    }
    for content in source_files.values():
        scan_sensitive(content, private_markers)
    for content in binary_files.values():
        scan_sensitive(content, private_markers)

    commit = git(source, "rev-parse", "HEAD").decode("ascii").strip()
    return {
        "schema_version": 1,
        "source_commit": commit,
        "binary_archive_sha256": sha256(binary_archive),
        "source_archive_sha256": sha256(source_archive),
        "binary_file_count": len(binary_files),
        "source_file_count": len(source_files),
        "binary_executables": sorted(binary_executables),
        "source_executable_count": len(source_executables),
        "credential_signatures_found": 0,
        "private_local_paths_found": 0,
        "runtime_or_build_residue_found": 0,
        "undeclared_files_found": 0,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--binary-archive", type=Path, required=True)
    parser.add_argument("--source-archive", type=Path, required=True)
    arguments = parser.parse_args()
    report = scan(
        arguments.source.resolve(),
        arguments.binary_archive.resolve(),
        arguments.source_archive.resolve(),
    )
    print(json.dumps(report, sort_keys=True, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
