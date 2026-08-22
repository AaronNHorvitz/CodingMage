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
PACKAGE_SPEC = importlib.util.spec_from_file_location(
    "package_release", ROOT / "scripts" / "package_release.py"
)
assert PACKAGE_SPEC is not None and PACKAGE_SPEC.loader is not None
PACKAGER = importlib.util.module_from_spec(PACKAGE_SPEC)
PACKAGE_SPEC.loader.exec_module(PACKAGER)


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


class FakeController:
    def __init__(self) -> None:
        self.calls: list[tuple[str, ...]] = []

    def __call__(self, *arguments: str) -> None:
        self.calls.append(arguments)


class ReleaseToolsTest(unittest.TestCase):
    def test_local_build_paths_are_rejected_without_printing_content(self) -> None:
        with tempfile.TemporaryDirectory(prefix="codingmage-packager-test-") as temporary:
            binary = Path(temporary) / "binary"
            binary.write_bytes(f"prefix {PACKAGER.ROOT} suffix".encode())
            with self.assertRaisesRegex(RuntimeError, "local build path"):
                PACKAGER.reject_local_paths(binary)

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

    def test_packaged_service_install_start_upgrade_rollback_stop_and_remove(self) -> None:
        with tempfile.TemporaryDirectory(prefix="codingmage-service-test-") as temporary:
            root = Path(temporary)
            prefix = root / "prefix"
            unit_root = root / "units"
            state = root / "state"
            scratch = root / "scratch"
            state.mkdir()
            scratch.mkdir()
            configuration = root / "config.toml"
            configuration.write_text(
                f'state_root = "{state}"\nscratch_root = "{scratch}"\n', encoding="utf-8"
            )
            campaign = root / "campaign.toml"
            campaign.write_text("version = 3\n", encoding="utf-8")
            first = archive(root, "service-first", b"first-service-binary")
            second = archive(root, "service-second", b"second-service-binary")
            controller = FakeController()

            INSTALLER.install(first, prefix)
            INSTALLER.install_service(
                prefix, unit_root, configuration, campaign, controller
            )
            INSTALLER.verify_service(prefix, unit_root)
            unit, receipt = INSTALLER.service_paths(prefix, unit_root)
            unit_content = unit.read_text(encoding="utf-8")
            self.assertIn(" campaign --config ", unit_content)
            self.assertIn(" --campaign ", unit_content)
            self.assertNotIn(" run --config ", unit_content)

            INSTALLER.control_service(prefix, unit_root, "start", controller)
            INSTALLER.control_service(prefix, unit_root, "stop", controller)
            INSTALLER.install(second, prefix)
            INSTALLER.verify_service(prefix, unit_root)
            INSTALLER.rollback(prefix)
            INSTALLER.verify_service(prefix, unit_root)
            INSTALLER.control_service(prefix, unit_root, "start", controller)
            INSTALLER.remove_service(prefix, unit_root, controller)
            INSTALLER.remove_service(prefix, unit_root, controller)

            self.assertFalse(unit.exists())
            self.assertFalse(receipt.exists())
            self.assertEqual(
                controller.calls,
                [
                    ("daemon-reload",),
                    ("start", "codingmage.service"),
                    ("stop", "codingmage.service"),
                    ("start", "codingmage.service"),
                    ("stop", "codingmage.service"),
                    ("daemon-reload",),
                ],
            )

    def test_changed_or_linked_service_state_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory(prefix="codingmage-service-test-") as temporary:
            root = Path(temporary)
            prefix = root / "prefix"
            unit_root = root / "units"
            state = root / "state"
            scratch = root / "scratch"
            state.mkdir()
            scratch.mkdir()
            configuration = root / "config.toml"
            configuration.write_text(
                f'state_root = "{state}"\nscratch_root = "{scratch}"\n', encoding="utf-8"
            )
            campaign = root / "campaign.toml"
            campaign.write_text("version = 3\n", encoding="utf-8")
            INSTALLER.install(archive(root, "service", b"binary"), prefix)
            controller = FakeController()
            INSTALLER.install_service(
                prefix, unit_root, configuration, campaign, controller
            )
            unit, _ = INSTALLER.service_paths(prefix, unit_root)
            unit.write_text("human change\n", encoding="utf-8")
            with self.assertRaises(ValueError):
                INSTALLER.verify_service(prefix, unit_root)
            with self.assertRaises(ValueError):
                INSTALLER.remove_service(prefix, unit_root, controller)


if __name__ == "__main__":
    unittest.main()
