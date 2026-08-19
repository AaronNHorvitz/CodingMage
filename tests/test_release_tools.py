"""Isolated tests for rootless release installation lifecycle."""

from __future__ import annotations

import hashlib
import importlib.util
import io
import tarfile
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "install_release", ROOT / "scripts" / "install_release.py"
)
assert SPEC is not None and SPEC.loader is not None
INSTALLER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(INSTALLER)


def archive(root: Path, name: str, content: bytes) -> Path:
    bundle = root / f"{name}.tar.gz"
    digest = hashlib.sha256(content).hexdigest()
    with tarfile.open(bundle, "w:gz") as stream:
        binary = tarfile.TarInfo(f"codingmage-0.1.0/bin/codingmage")
        binary.mode = 0o755
        binary.size = len(content)
        stream.addfile(binary, io.BytesIO(content))
        checksums = f"{digest}  bin/codingmage\n".encode()
        manifest = tarfile.TarInfo("codingmage-0.1.0/SHA256SUMS")
        manifest.size = len(checksums)
        stream.addfile(manifest, io.BytesIO(checksums))
    return bundle


class ReleaseToolsTest(unittest.TestCase):
    def test_install_upgrade_verify_rollback_remove_preserves_unrelated_data(self) -> None:
        with tempfile.TemporaryDirectory(prefix="codingmage-installer-test-") as temporary:
            root = Path(temporary)
            prefix = root / "prefix"
            unrelated = prefix / "keep.txt"
            prefix.mkdir()
            unrelated.write_text("preserve\n", encoding="utf-8")
            first = archive(root, "first", b"first-binary")
            second = archive(root, "second", b"second-binary")

            INSTALLER.install(first, prefix)
            INSTALLER.verify(prefix)
            binary, previous, _ = INSTALLER.paths(prefix)
            self.assertEqual(binary.read_bytes(), b"first-binary")
            self.assertFalse(previous.exists())

            INSTALLER.install(second, prefix)
            INSTALLER.verify(prefix)
            self.assertEqual(binary.read_bytes(), b"second-binary")
            self.assertEqual(previous.read_bytes(), b"first-binary")

            INSTALLER.rollback(prefix)
            INSTALLER.verify(prefix)
            self.assertEqual(binary.read_bytes(), b"first-binary")
            self.assertEqual(previous.read_bytes(), b"second-binary")

            INSTALLER.remove(prefix, False)
            INSTALLER.remove(prefix, False)
            self.assertFalse(binary.exists())
            self.assertEqual(unrelated.read_text(encoding="utf-8"), "preserve\n")

    def test_checksum_and_archive_traversal_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory(prefix="codingmage-installer-test-") as temporary:
            root = Path(temporary)
            changed = root / "changed.tar.gz"
            with tarfile.open(changed, "w:gz") as stream:
                content = b"changed"
                binary = tarfile.TarInfo("codingmage-0.1.0/bin/codingmage")
                binary.size = len(content)
                stream.addfile(binary, io.BytesIO(content))
                checksums = f"{'0' * 64}  bin/codingmage\n".encode()
                manifest = tarfile.TarInfo("codingmage-0.1.0/SHA256SUMS")
                manifest.size = len(checksums)
                stream.addfile(manifest, io.BytesIO(checksums))
            with self.assertRaises(ValueError):
                INSTALLER.install(changed, root / "prefix")

            traversal = root / "traversal.tar.gz"
            with tarfile.open(traversal, "w:gz") as stream:
                member = tarfile.TarInfo("../escape")
                member.size = 1
                stream.addfile(member, io.BytesIO(b"x"))
            extract = root / "extract"
            extract.mkdir()
            with self.assertRaises(ValueError):
                INSTALLER.safe_extract(traversal, extract)


if __name__ == "__main__":
    unittest.main()
