"""Tests for deterministic release artifact inspection."""

from __future__ import annotations

import hashlib
import importlib.util
import io
import json
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "scan_release", ROOT / "scripts" / "scan_release.py"
)
assert SPEC is not None and SPEC.loader is not None
SCANNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SCANNER)


def add_file(bundle: tarfile.TarFile, name: str, content: bytes, mode: int = 0o644) -> None:
    member = tarfile.TarInfo(name)
    member.mode = mode
    member.size = len(content)
    bundle.addfile(member, io.BytesIO(content))


def source_fixture(root: Path) -> tuple[Path, Path]:
    source = root / "source"
    source.mkdir()
    subprocess.run(["/usr/bin/git", "init", "-q", str(source)], check=True)
    subprocess.run(
        ["/usr/bin/git", "-C", str(source), "config", "user.name", "Fixture"],
        check=True,
    )
    subprocess.run(
        [
            "/usr/bin/git",
            "-C",
            str(source),
            "config",
            "user.email",
            "fixture@example.invalid",
        ],
        check=True,
    )
    tracked = source / "tracked.txt"
    tracked.write_text("tracked\n", encoding="utf-8")
    subprocess.run(["/usr/bin/git", "-C", str(source), "add", "tracked.txt"], check=True)
    subprocess.run(
        ["/usr/bin/git", "-C", str(source), "commit", "-q", "-m", "fixture"],
        check=True,
    )
    source_archive = root / "source.tar.gz"
    with tarfile.open(source_archive, "w:gz") as bundle:
        add_file(bundle, f"{SCANNER.SOURCE_ROOT}/tracked.txt", tracked.read_bytes())
    return source, source_archive


def binary_fixture(root: Path, override: tuple[str, bytes, int] | None = None) -> Path:
    content = {name: f"content:{name}\n".encode() for name in SCANNER.BINARY_FILES}
    content["bin/codingmage"] = b"binary"
    if override is not None:
        name, value, _ = override
        content[name] = value
    checksums = "".join(
        f"{hashlib.sha256(value).hexdigest()}  {name}\n"
        for name, value in sorted(content.items())
        if name != "SHA256SUMS"
    ).encode()
    content["SHA256SUMS"] = checksums
    archive = root / "binary.tar.gz"
    with tarfile.open(archive, "w:gz") as bundle:
        for name, value in sorted(content.items()):
            mode = 0o755 if name == "bin/codingmage" else 0o644
            if override is not None and name == override[0]:
                mode = override[2]
            add_file(bundle, f"{SCANNER.BINARY_ROOT}/{name}", value, mode)
    return archive


class ReleaseScanTest(unittest.TestCase):
    def test_exact_archives_pass_with_content_minimized_report(self) -> None:
        with tempfile.TemporaryDirectory(prefix="codingmage-release-scan-") as temporary:
            root = Path(temporary)
            source, source_archive = source_fixture(root)
            report = SCANNER.scan(source, binary_fixture(root), source_archive)
            self.assertEqual(report["binary_file_count"], len(SCANNER.BINARY_FILES))
            self.assertEqual(report["binary_executables"], ["bin/codingmage"])
            self.assertEqual(report["credential_signatures_found"], 0)
            self.assertNotIn(str(root), json.dumps(report))

    def test_credentials_private_paths_and_extra_executables_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory(prefix="codingmage-release-scan-") as temporary:
            root = Path(temporary)
            source, source_archive = source_fixture(root)
            token = "gh" + "p_" + "A" * 24
            credential = binary_fixture(
                root, ("share/doc/codingmage/README.md", token.encode(), 0o644)
            )
            with self.assertRaisesRegex(ValueError, "credential signature"):
                SCANNER.scan(source, credential, source_archive)

            private = binary_fixture(
                root, ("share/doc/codingmage/README.md", str(source).encode(), 0o644)
            )
            with self.assertRaisesRegex(ValueError, "private local path"):
                SCANNER.scan(source, private, source_archive)

            executable = binary_fixture(
                root, ("share/doc/codingmage/README.md", b"text", 0o755)
            )
            with self.assertRaisesRegex(ValueError, "executable inventory"):
                SCANNER.scan(source, executable, source_archive)

    def test_dirty_source_and_undeclared_archive_content_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory(prefix="codingmage-release-scan-") as temporary:
            root = Path(temporary)
            source, source_archive = source_fixture(root)
            (source / "untracked.txt").write_text("dirty\n", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "source is dirty"):
                SCANNER.scan(source, binary_fixture(root), source_archive)

            (source / "untracked.txt").unlink()
            binary = binary_fixture(root)
            changed = root / "changed.tar.gz"
            with tarfile.open(binary, "r:gz") as original, tarfile.open(changed, "w:gz") as output:
                for member in original.getmembers():
                    stream = original.extractfile(member) if member.isfile() else None
                    output.addfile(member, stream)
                add_file(output, f"{SCANNER.BINARY_ROOT}/unexpected.txt", b"unexpected")
            with self.assertRaisesRegex(ValueError, "undeclared or missing"):
                SCANNER.scan(source, changed, source_archive)


if __name__ == "__main__":
    unittest.main()
