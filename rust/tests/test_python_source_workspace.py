"""Protect locked source rebuilds from maturin's workspace pruning."""

import importlib.util
import io
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "yamaa_build_backend", Path(__file__).parents[2] / "python/yamaa_build_backend.py"
)
backend = importlib.util.module_from_spec(spec)
spec.loader.exec_module(backend)

guard_spec = importlib.util.spec_from_file_location(
    "check_cargo_lock", Path(__file__).parents[1] / "tools/check_cargo_lock.py"
)
guard = importlib.util.module_from_spec(guard_spec)
guard_spec.loader.exec_module(guard)


def write_archive(path, values):
    with tarfile.open(path, "w:gz") as archive:
        for name, payload in values:
            member = tarfile.TarInfo(name)
            member.size = len(payload)
            archive.addfile(member, io.BytesIO(payload))


class PythonSourceWorkspaceTests(unittest.TestCase):
    def test_pruned_archive_is_restored_and_resolves_without_rewriting_lock(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            workspace = root / "rust"
            workspace.mkdir()
            manifest = b'[workspace]\nmembers = ["crates/python", "crates/r"]\nresolver = "2"\n'
            (workspace / "Cargo.toml").write_bytes(manifest)
            for member in ("python", "r"):
                crate = workspace / "crates" / member
                (crate / "src").mkdir(parents=True)
                (crate / "Cargo.toml").write_text(
                    f'[package]\nname = "witness-{member}"\nversion = "0.1.0"\nedition = "2021"\n'
                )
                (crate / "src/lib.rs").write_text("pub fn unchanged() {}\n")
            subprocess.run(
                ["cargo", "generate-lockfile", "--offline"],
                cwd=workspace,
                check=True,
                capture_output=True,
            )
            lock = (workspace / "Cargo.lock").read_bytes()
            archive = root / "yamaa.tar.gz"
            values = [
                ("yamaa/rust/Cargo.toml", manifest.replace(b', "crates/r"', b"")),
                ("yamaa/rust/Cargo.lock", lock),
                ("yamaa/pyproject.toml", b"independent Python metadata\n"),
            ]
            values += [
                (
                    f"yamaa/rust/crates/python/{name}",
                    (workspace / "crates/python" / name).read_bytes(),
                )
                for name in ("Cargo.toml", "src/lib.rs")
            ]
            write_archive(archive, values)
            with self.assertRaisesRegex(ValueError, "manifest differs"):
                guard.check_archive(archive, lock, manifest)
            for name in ("target/generated.rs", "__pycache__/cache.pyc", "Cargo.lock"):
                path = workspace / "crates/r" / name
                path.parent.mkdir(exist_ok=True)
                path.write_bytes(b"generated")
            backend.preserve_workspace(archive, workspace)
            guard.check_archive(archive, lock, manifest)
            with tarfile.open(archive) as result:
                self.assertEqual(
                    result.extractfile("yamaa/rust/Cargo.toml").read(), manifest
                )
                self.assertEqual(
                    result.extractfile("yamaa/rust/Cargo.lock").read(), lock
                )
                self.assertEqual(
                    result.extractfile("yamaa/pyproject.toml").read(), values[2][1]
                )
                self.assertEqual(
                    result.extractfile("yamaa/rust/crates/r/src/lib.rs").read(),
                    (workspace / "crates/r/src/lib.rs").read_bytes(),
                )
                self.assertEqual(len(result.getnames()), len(values) + 2)
                result.extractall(root / "unpacked", filter="data")
            restored = root / "unpacked/yamaa/rust"
            subprocess.run(
                ["cargo", "metadata", "--locked", "--offline", "--format-version", "1"],
                cwd=restored,
                check=True,
                capture_output=True,
            )
            self.assertEqual((restored / "Cargo.lock").read_bytes(), lock)

    def test_atomic_replacement_closes_archive_handles_first(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest = b"[workspace]\nmembers = []\n"
            (root / "Cargo.toml").write_bytes(manifest)
            (root / "Cargo.lock").write_bytes(b"canonical")
            archive = root / "yamaa.tar.gz"
            write_archive(
                archive,
                [
                    ("yamaa/rust/Cargo.toml", b"pruned"),
                    ("yamaa/rust/Cargo.lock", b"canonical"),
                    ("yamaa/held.txt", b"unchanged payload"),
                ],
            )
            handles = []
            original_open = tarfile.open
            original_replace = backend.os.replace
            replacements = []

            def tracked_open(*args, **kwargs):
                handle = original_open(*args, **kwargs)
                handles.append(handle)
                return handle

            def replace_closed(source, destination):
                self.assertEqual(len(handles), 2)
                self.assertTrue(
                    all(handle.closed and handle.fileobj.closed for handle in handles)
                )
                replacements.append(destination)
                original_replace(source, destination)

            with (
                patch.object(backend.tarfile, "open", side_effect=tracked_open),
                patch.object(backend.os, "replace", side_effect=replace_closed),
            ):
                backend.preserve_workspace(archive, root)
            self.assertEqual(replacements, [archive])
            with tarfile.open(archive) as result:
                self.assertEqual(
                    result.extractfile("yamaa/rust/Cargo.toml").read(), manifest
                )
                self.assertEqual(
                    result.extractfile("yamaa/held.txt").read(), b"unchanged payload"
                )
            self.assertEqual(list(root.glob("*.tar.gz")), [archive])

    def test_contradictory_archive_refuses_without_replacing_original(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text("[workspace]\nmembers = []\n")
            (root / "Cargo.lock").write_bytes(b"canonical")
            archive = root / "yamaa.tar.gz"
            write_archive(
                archive,
                [
                    ("yamaa/rust/Cargo.toml", b"manifest"),
                    ("yamaa/rust/Cargo.lock", b"other"),
                ],
            )
            original = archive.read_bytes()
            with self.assertRaisesRegex(ValueError, "changed"):
                backend.preserve_workspace(archive, root)
            self.assertEqual(archive.read_bytes(), original)
            self.assertEqual(list(root.glob("*.tar.gz")), [archive])


if __name__ == "__main__":
    unittest.main()
