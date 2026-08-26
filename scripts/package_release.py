#!/usr/bin/env python3
"""Build and package a deterministic local CodingMage release candidate."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import io
import json
import os
import re
import shutil
import subprocess
import tarfile
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
VERSION = "0.1.0"
SHA256 = re.compile(r"^[0-9a-f]{64}$")
COMMIT = re.compile(r"^[0-9a-f]{40}$")
REVIEW_FIELDS = {
    "schema_version",
    "source_commit",
    "reviewed_commit",
    "disposition",
    "evidence",
}


def run(*arguments: str, cwd: Path = ROOT) -> bytes:
    home = Path.home()
    remaps = {
        ROOT.resolve(): Path("/usr/src/codingmage"),
        home: Path("/build/home"),
        home.resolve(): Path("/build/home"),
    }
    environment = {
        "HOME": os.environ.get("HOME", "/nonexistent"),
        "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
        "SOURCE_DATE_EPOCH": source_epoch(),
        "CARGO_INCREMENTAL": "0",
        "RUSTFLAGS": " ".join(
            f"--remap-path-prefix={source}={destination}"
            for source, destination in sorted(remaps.items(), key=lambda item: str(item[0]))
        ),
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


def git(*arguments: str) -> bytes:
    return subprocess.run(
        ["/usr/bin/git", *arguments],
        cwd=ROOT,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    ).stdout


def verify_release_source(review_record: Path) -> tuple[str, str]:
    """Require clean source and one external exact-commit review record."""
    if git("status", "--porcelain=v1", "--untracked-files=all"):
        raise RuntimeError("release source is dirty")
    source_commit = git("rev-parse", "HEAD").decode("ascii").strip()
    if not COMMIT.fullmatch(source_commit):
        raise RuntimeError("release source commit is invalid")
    if not review_record.is_absolute() or not review_record.is_file() or review_record.is_symlink():
        raise RuntimeError("release review record is invalid")
    resolved_record = review_record.resolve(strict=True)
    resolved_root = ROOT.resolve(strict=True)
    if resolved_record == resolved_root or resolved_root in resolved_record.parents:
        raise RuntimeError("release review record must be external")
    try:
        document = json.loads(resolved_record.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise RuntimeError("release review record is invalid") from error
    if not isinstance(document, dict) or set(document) != REVIEW_FIELDS:
        raise RuntimeError("release review record is invalid")
    if (
        document["schema_version"] != 1
        or document["source_commit"] != source_commit
        or document["reviewed_commit"] != source_commit
        or document["disposition"] != "approved_for_candidate_construction"
        or not isinstance(document["evidence"], list)
        or not document["evidence"]
    ):
        raise RuntimeError("release review record does not bind source")
    seen: set[str] = set()
    for item in document["evidence"]:
        if not isinstance(item, dict) or set(item) != {"path", "sha256"}:
            raise RuntimeError("release evidence entry is invalid")
        path = item["path"]
        digest = item["sha256"]
        if (
            not isinstance(path, str)
            or not path
            or Path(path).is_absolute()
            or ".." in Path(path).parts
            or path in seen
            or not isinstance(digest, str)
            or not SHA256.fullmatch(digest)
        ):
            raise RuntimeError("release evidence entry is invalid")
        seen.add(path)
        candidate = ROOT / path
        if not candidate.is_file() or candidate.is_symlink() or sha256(candidate) != digest:
            raise RuntimeError("release evidence does not match source")
    return source_commit, sha256(resolved_record)


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
    reject_local_paths(binary)
    return binary


def reject_local_paths(binary: Path) -> None:
    content = binary.read_bytes()
    candidates = {
        str(ROOT),
        str(ROOT.resolve()),
        str(Path.home()),
        str(Path.home().resolve()),
    }
    for candidate in candidates:
        if candidate and candidate.encode() in content:
            raise RuntimeError("release binary contains a local build path")


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


def write_dependency_inventory(destination: Path, cargo_metadata: dict[str, object]) -> None:
    packages = [
        {
            "name": package["name"],
            "version": package["version"],
            "license": package.get("license") or "NOASSERTION",
            "source": package.get("source") or "workspace",
        }
        for package in sorted(
            cargo_metadata["packages"], key=lambda item: (item["name"], item["version"])
        )
    ]
    destination.write_text(
        json.dumps({"schema_version": 1, "packages": packages}, sort_keys=True, indent=2)
        + "\n",
        encoding="utf-8",
    )


def write_provenance(
    destination: Path, source_commit: str, review_sha256: str, binary_sha256: str
) -> None:
    document = {
        "schema_version": 1,
        "source_commit": source_commit,
        "source_date_epoch": int(source_epoch()),
        "cargo_lock_sha256": sha256(ROOT / "Cargo.lock"),
        "rust_toolchain_sha256": sha256(ROOT / "rust-toolchain.toml"),
        "release_review_sha256": review_sha256,
        "binary_sha256": binary_sha256,
        "reproducible_binary_builds": 2,
        "publication_authorized": False,
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


def deterministic_source_archive(destination: Path) -> None:
    epoch = int(source_epoch())
    tracked = [
        item.decode("utf-8")
        for item in git("ls-files", "-z").split(b"\0")
        if item
    ]
    if not tracked:
        raise RuntimeError("release source inventory is empty")
    with destination.open("wb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode="w") as archive:
                for name in sorted(tracked):
                    path = ROOT / name
                    if not path.is_file() or path.is_symlink():
                        raise RuntimeError("release source contains unsupported tracked entry")
                    relative = Path(f"codingmage-{VERSION}-source") / name
                    info = archive.gettarinfo(str(path), arcname=str(relative))
                    info.uid = 0
                    info.gid = 0
                    info.uname = "root"
                    info.gname = "root"
                    info.mtime = epoch
                    with path.open("rb") as stream:
                        archive.addfile(info, stream)


def package(output: Path, review_record: Path) -> Path:
    source_commit, review_sha256 = verify_release_source(review_record)
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
        for name in [
            "LICENSE",
            "README.md",
            "RELEASE-NOTES.md",
            "SECURITY.md",
            "SUPPORT.md",
            "THIRD-PARTY-NOTICES.md",
        ]:
            shutil.copy2(ROOT / name, layout / "share" / "doc" / "codingmage" / name)
        cargo_metadata = metadata()
        write_sbom(layout / "SBOM.spdx.json", cargo_metadata)
        write_dependency_inventory(layout / "DEPENDENCIES.json", cargo_metadata)
        binary_sha256 = sha256(layout / "bin" / "codingmage")
        write_provenance(
            layout / "PROVENANCE.json", source_commit, review_sha256, binary_sha256
        )
        manifest = {
            "schema_version": 1,
            "version": VERSION,
            "source_commit": source_commit,
            "source_date_epoch": int(source_epoch()),
            "cargo_lock_sha256": sha256(ROOT / "Cargo.lock"),
            "binary_sha256": binary_sha256,
            "release_review_sha256": review_sha256,
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
        source_archive = output / f"codingmage-{VERSION}-source.tar.gz"
        deterministic_source_archive(source_archive)
        (output / f"{archive.name}.sha256").write_text(
            f"{sha256(archive)}  {archive.name}\n", encoding="ascii"
        )
        release_manifest = {
            "schema_version": 1,
            "version": VERSION,
            "source_commit": source_commit,
            "release_review_sha256": review_sha256,
            "artifacts": [
                {"name": archive.name, "sha256": sha256(archive)},
                {"name": source_archive.name, "sha256": sha256(source_archive)},
            ],
            "signed": False,
            "published": False,
        }
        (output / "RELEASE-MANIFEST.json").write_text(
            json.dumps(release_manifest, sort_keys=True, indent=2) + "\n", encoding="utf-8"
        )
        (output / "SHA256SUMS").write_text(
            "".join(
                f"{item['sha256']}  {item['name']}\n"
                for item in release_manifest["artifacts"]
            ),
            encoding="ascii",
        )
        return archive


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "dist")
    parser.add_argument("--review-record", type=Path, required=True)
    args = parser.parse_args()
    archive = package(args.output.resolve(), args.review_record)
    print(archive)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
