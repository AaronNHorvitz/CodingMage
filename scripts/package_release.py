#!/usr/bin/env python3
"""Build and package a deterministic local CodingMage release candidate."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import io
import json
import os
import shutil
import subprocess
import tarfile
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
VERSION = "0.1.0"


def run(*arguments: str, cwd: Path = ROOT) -> bytes:
    environment = {
        "HOME": os.environ.get("HOME", "/nonexistent"),
        "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
        "SOURCE_DATE_EPOCH": source_epoch(),
        "CARGO_INCREMENTAL": "0",
    }
    if "CARGO_TARGET_DIR" in os.environ:
        environment["CARGO_TARGET_DIR"] = os.environ["CARGO_TARGET_DIR"]
    return subprocess.run(
        arguments,
        cwd=cwd,
        env=environment,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    ).stdout


def source_epoch() -> str:
    return subprocess.run(
        ["/usr/bin/git", "show", "-s", "--format=%ct", "HEAD"],
        cwd=ROOT,
        check=True,
        stdout=subprocess.PIPE,
        text=True,
    ).stdout.strip()


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def metadata() -> dict[str, object]:
    return json.loads(run("cargo", "metadata", "--locked", "--format-version", "1"))


def build_binary(target_dir: Path) -> Path:
    environment_target = str(target_dir)
    previous = os.environ.get("CARGO_TARGET_DIR")
    os.environ["CARGO_TARGET_DIR"] = environment_target
    try:
        run("cargo", "build", "--locked", "--release", "-p", "codingmage-cli")
    finally:
        if previous is None:
            os.environ.pop("CARGO_TARGET_DIR", None)
        else:
            os.environ["CARGO_TARGET_DIR"] = previous
    binary = target_dir / "release" / "codingmage"
    if not binary.is_file():
        raise RuntimeError("release binary is missing")
    return binary


def write_sbom(destination: Path, cargo_metadata: dict[str, object]) -> None:
    packages = []
    for package in sorted(cargo_metadata["packages"], key=lambda item: (item["name"], item["version"])):
        packages.append(
            {
                "SPDXID": f"SPDXRef-Package-{package['name']}-{package['version']}",
                "name": package["name"],
                "versionInfo": package["version"],
                "licenseConcluded": package.get("license") or "NOASSERTION",
                "downloadLocation": package.get("source") or "NOASSERTION",
                "filesAnalyzed": False,
            }
        )
    document = {
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": f"CodingMage-{VERSION}",
        "documentNamespace": f"https://github.com/AaronNHorvitz/CodingMage/sbom/{commit()}",
        "creationInfo": {
            "created": "1970-01-01T00:00:00Z",
            "creators": ["Tool: CodingMage-package_release"],
        },
        "packages": packages,
    }
    destination.write_text(json.dumps(document, sort_keys=True, indent=2) + "\n", encoding="utf-8")


def commit() -> str:
    return subprocess.run(
        ["/usr/bin/git", "rev-parse", "HEAD"],
        cwd=ROOT,
        check=True,
        stdout=subprocess.PIPE,
        text=True,
    ).stdout.strip()


def deterministic_archive(source: Path, destination: Path) -> None:
    epoch = int(source_epoch())
    with destination.open("wb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode="w") as archive:
                for path in sorted(source.rglob("*")):
                    relative = Path(f"codingmage-{VERSION}") / path.relative_to(source)
                    info = archive.gettarinfo(str(path), arcname=str(relative))
                    info.uid = 0
                    info.gid = 0
                    info.uname = "root"
                    info.gname = "root"
                    info.mtime = epoch
                    if path.is_file():
                        with path.open("rb") as stream:
                            archive.addfile(info, stream)
                    else:
                        archive.addfile(info)


def package(output: Path) -> Path:
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="codingmage-package-") as temporary:
        temporary_path = Path(temporary)
        first = build_binary(temporary_path / "build-one")
        second = build_binary(temporary_path / "build-two")
        if sha256(first) != sha256(second):
            raise RuntimeError("two clean release builds are not reproducible")

        layout = temporary_path / "layout"
        (layout / "bin").mkdir(parents=True)
        (layout / "share" / "doc" / "codingmage").mkdir(parents=True)
        shutil.copy2(first, layout / "bin" / "codingmage")
        for name in ["LICENSE", "README.md", "SECURITY.md"]:
            shutil.copy2(ROOT / name, layout / "share" / "doc" / "codingmage" / name)
        write_sbom(layout / "SBOM.spdx.json", metadata())
        manifest = {
            "schema_version": 1,
            "version": VERSION,
            "source_commit": commit(),
            "source_date_epoch": int(source_epoch()),
            "cargo_lock_sha256": sha256(ROOT / "Cargo.lock"),
            "binary_sha256": sha256(layout / "bin" / "codingmage"),
            "contains_credentials": False,
            "contains_runtime_state": False,
            "native_evidence": "linux-only",
        }
        (layout / "BUILD-MANIFEST.json").write_text(
            json.dumps(manifest, sort_keys=True, indent=2) + "\n", encoding="utf-8"
        )
        checksums = []
        for path in sorted(item for item in layout.rglob("*") if item.is_file()):
            checksums.append(f"{sha256(path)}  {path.relative_to(layout).as_posix()}")
        (layout / "SHA256SUMS").write_text("\n".join(checksums) + "\n", encoding="ascii")
        archive = output / f"codingmage-{VERSION}-linux-x86_64.tar.gz"
        deterministic_archive(layout, archive)
        (output / f"{archive.name}.sha256").write_text(
            f"{sha256(archive)}  {archive.name}\n", encoding="ascii"
        )
        return archive


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "dist")
    args = parser.parse_args()
    archive = package(args.output.resolve())
    print(archive)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
