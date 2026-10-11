"""Exercise real Cargo refusal and both source archive lock layouts."""

import importlib.util
import io
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "check_cargo_lock", Path(__file__).parents[1] / "tools/check_cargo_lock.py"
)
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)


def archive(path, entries):
    with tarfile.open(path, "w:gz") as output:
        for name, value in entries:
            member = tarfile.TarInfo(name)
            if isinstance(value, bytes):
                member.size = len(value)
                output.addfile(member, io.BytesIO(value))
            else:
                member.type = tarfile.SYMTYPE
                member.linkname = value
                output.addfile(member)


class CargoLockTests(unittest.TestCase):
    def test_missing_and_stale_locks_refuse_without_changing_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace = Path(directory)
            (workspace / "src").mkdir()
            (workspace / "src/lib.rs").write_text("pub fn unchanged() {}\n")
            manifest = workspace / "Cargo.toml"
            manifest.write_text(
                '[package]\nname = "lock-witness"\nversion = "0.1.0"\nedition = "2021"\n'
            )
            with self.assertRaisesRegex(ValueError, "missing"):
                guard.locked_metadata(workspace)
            self.assertFalse((workspace / "Cargo.lock").exists())
            subprocess.run(
                ["cargo", "generate-lockfile", "--offline"],
                cwd=workspace,
                check=True,
                capture_output=True,
            )
            lock = (workspace / "Cargo.lock").read_bytes()
            guard.locked_metadata(workspace)
            manifest.write_text(manifest.read_text().replace('"0.1.0"', '"0.2.0"'))
            with self.assertRaises(subprocess.CalledProcessError):
                guard.locked_metadata(workspace)
            self.assertEqual((workspace / "Cargo.lock").read_bytes(), lock)

    def test_both_source_packages_require_exact_canonical_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "yamaa.tar.gz"
            for location in (
                "yamaa-0.3.0/rust/Cargo.lock",
                "yamaa/src/rust/Cargo.lock",
            ):
                with self.subTest(location=location):
                    manifest_name = str(Path(location).with_name("Cargo.toml")).replace(
                        "\\", "/"
                    )
                    archive(
                        path,
                        [
                            (location, b"canonical lock\n"),
                            (manifest_name, b"workspace\n"),
                        ],
                    )
                    guard.check_archive(path, b"canonical lock\n", b"workspace\n")
                    with self.assertRaisesRegex(ValueError, "differs"):
                        guard.check_archive(path, b"contradictory\n", b"workspace\n")
                    with self.assertRaisesRegex(ValueError, "manifest differs"):
                        guard.check_archive(
                            path, b"canonical lock\n", b"pruned workspace\n"
                        )

    def test_missing_duplicate_misplaced_and_linked_locks_refuse(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "yamaa.tar.gz"
            for entries, message in (
                ([], "exactly one"),
                (
                    [
                        ("yamaa/rust/Cargo.lock", b"lock"),
                        ("yamaa/rust/Cargo.lock", b"lock"),
                    ],
                    "exactly one",
                ),
                ([("yamaa/Cargo.lock", b"lock")], "unexpected"),
                ([("yamaa/rust/Cargo.lock", "elsewhere")], "regular file"),
            ):
                with self.subTest(entries=entries):
                    archive(path, entries)
                    with self.assertRaisesRegex(ValueError, message):
                        guard.check_archive(path, b"lock", b"workspace\n")


if __name__ == "__main__":
    unittest.main()
