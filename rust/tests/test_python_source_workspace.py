"""Protect locked source rebuilds from maturin's workspace pruning."""

import importlib.util
import io
import json
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
    def test_absolute_and_drive_compile_paths_cannot_read_the_original_checkout(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            workspace = root / "rust"
            manifest = workspace / "qualification/native-source-fixtures.json"
            manifest.parent.mkdir(parents=True)
            name = "benchmarks/original/expected/out.csv"
            original = root / name
            original.parent.mkdir(parents=True)
            original.write_bytes(b"ID\n1\n")
            manifest.write_text(
                json.dumps(
                    {"format": "yamaa-native-source-fixtures/1", "files": [name]}
                )
            )
            source = workspace / "crates/engine/tests/original.rs"
            source.parent.mkdir(parents=True)
            for path in (
                str(original.resolve()),
                "C:/original/out.csv",
                r"C:\original\out.csv",
            ):
                with self.subTest(path=path):
                    source.write_text(
                        f"const INPUT: &str = include_str!({json.dumps(path)});\n"
                    )
                    with self.assertRaisesRegex(ValueError, "unescaped relative paths"):
                        backend.native_test_fixtures(workspace)

    def test_compile_symlink_alias_is_not_replaced_by_its_declared_regular_target(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            workspace = root / "rust"
            manifest = workspace / "qualification/native-source-fixtures.json"
            manifest.parent.mkdir(parents=True)
            name = "benchmarks/original/expected/out.csv"
            original = root / name
            original.parent.mkdir(parents=True)
            original.write_bytes(b"ID\n1\n")
            manifest.write_text(
                json.dumps(
                    {"format": "yamaa-native-source-fixtures/1", "files": [name]}
                )
            )
            alias = root / "benchmarks/alias"
            try:
                alias.symlink_to(original.parent, target_is_directory=True)
            except OSError as error:
                self.skipTest(f"compile-input symlink witness is unavailable: {error}")
            source = workspace / "crates/engine/tests/original.rs"
            source.parent.mkdir(parents=True)
            source.write_text(
                'const INPUT: &str = include_str!("../../../../benchmarks/alias/out.csv");\n'
            )
            with self.assertRaisesRegex(
                ValueError, "compile fixtures must not be symlinks"
            ):
                backend.native_test_fixtures(workspace)

    def test_external_compile_fixture_must_be_declared_before_building_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            workspace = root / "rust"
            manifest = workspace / "qualification/native-source-fixtures.json"
            manifest.parent.mkdir(parents=True)
            source = workspace / "crates/engine/tests/original.rs"
            source.parent.mkdir(parents=True)
            name = "benchmarks/original/expected/out.csv"
            original = root / name
            original.parent.mkdir(parents=True)
            original.write_bytes(b"ID\n1\n")
            source.write_text(
                "const ORIGINAL: &str = include_str!(\n"
                '    "../../../../benchmarks/original/expected/out.csv"\n);\n'
            )
            manifest.write_text(
                json.dumps({"format": "yamaa-native-source-fixtures/1", "files": []})
            )
            with self.assertRaisesRegex(ValueError, "compile fixture is not declared"):
                backend.native_test_fixtures(workspace)
            manifest.write_text(
                json.dumps(
                    {"format": "yamaa-native-source-fixtures/1", "files": [name]}
                )
            )
            selected = backend.native_test_fixtures(workspace)
            self.assertEqual(selected[1][0], name)
            self.assertEqual(selected[1][1].read_bytes(), b"ID\n1\n")

    def test_native_fixture_manifest_selects_exact_regular_original_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            workspace = root / "rust"
            manifest = workspace / "qualification/native-source-fixtures.json"
            manifest.parent.mkdir(parents=True)
            name = "benchmarks/independent/expected/document.xml"
            original = root / name
            original.parent.mkdir(parents=True)
            original.write_bytes(b"independent fixed-time document\n")
            (original.parent / "unselected.csv").write_bytes(b"not selected\n")
            manifest.write_text(
                json.dumps(
                    {"format": "yamaa-native-source-fixtures/1", "files": [name]}
                )
            )
            selected = backend.native_test_fixtures(workspace)
            self.assertEqual(
                [name for name, _ in selected],
                ["rust/qualification/native-source-fixtures.json", name],
            )
            self.assertEqual(
                selected[1][1].read_bytes(), b"independent fixed-time document\n"
            )
            original.unlink()
            with self.assertRaisesRegex(ValueError, "missing"):
                backend.native_test_fixtures(workspace)

    def test_native_fixture_manifest_refuses_ambiguous_or_unselected_paths(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace = Path(directory) / "rust"
            manifest = workspace / "qualification/native-source-fixtures.json"
            manifest.parent.mkdir(parents=True)
            for files in (
                ["../private"],
                ["/benchmarks/private"],
                ["benchmarks/../private"],
                ["benchmarks/a\\b"],
                ["benchmarks/a:stream"],
                ["benchmarks/a", "benchmarks/a"],
            ):
                with self.subTest(files=files):
                    manifest.write_text(
                        json.dumps(
                            {"format": "yamaa-native-source-fixtures/1", "files": files}
                        )
                    )
                    with self.assertRaisesRegex(ValueError, "fixture path"):
                        backend.native_test_fixtures(workspace)

    def test_source_fixture_restore_preserves_bytes_and_refuses_changed_truth(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            workspace = root / "rust"
            workspace.mkdir()
            manifest = b'[workspace]\nmembers = []\nresolver = "2"\n'
            lock = b"version = 4\n"
            toolchain = b'[toolchain]\nchannel = "1.90.0"\n'
            for name, payload in (
                ("Cargo.toml", manifest),
                ("Cargo.lock", lock),
                ("rust-toolchain.toml", toolchain),
            ):
                (workspace / name).write_bytes(payload)
            truth = root / "original.xml"
            truth.write_bytes(b"independently committed truth\n")
            archive = root / "yamaa.tar.gz"
            values = [
                ("yamaa/rust/Cargo.toml", manifest),
                ("yamaa/rust/Cargo.lock", lock),
            ]
            selected = [("benchmarks/independent/expected/document.xml", truth)]
            write_archive(archive, values)
            backend.preserve_workspace(archive, workspace, selected)
            with tarfile.open(archive) as result:
                self.assertEqual(
                    result.extractfile(
                        "yamaa/benchmarks/independent/expected/document.xml"
                    ).read(),
                    truth.read_bytes(),
                )
            write_archive(
                archive,
                values
                + [
                    (
                        "yamaa/benchmarks/independent/expected/document.xml",
                        b"changed truth\n",
                    )
                ],
            )
            before = archive.read_bytes()
            with self.assertRaisesRegex(ValueError, "changed a native test fixture"):
                backend.preserve_workspace(archive, workspace, selected)
            self.assertEqual(archive.read_bytes(), before)
            self.assertEqual(truth.read_bytes(), b"independently committed truth\n")

    def test_pruned_archive_is_restored_and_resolves_without_rewriting_lock(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            workspace = root / "rust"
            workspace.mkdir()
            manifest = b'[workspace]\nmembers = ["crates/python", "crates/r"]\nresolver = "2"\n'
            (workspace / "Cargo.toml").write_bytes(manifest)
            toolchain = b'[toolchain]\nchannel = "1.90.0"\n'
            (workspace / "rust-toolchain.toml").write_bytes(toolchain)
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
            for name in (
                "target/generated.rs",
                "__pycache__/cache.pyc",
                "Cargo.lock",
                "private.txt",
                "local.rs",
                "src/local-notes.txt",
            ):
                path = workspace / "crates/r" / name
                path.parent.mkdir(exist_ok=True)
                path.write_bytes(b"generated")
            backend.preserve_workspace(archive, workspace)
            guard.check_archive(archive, lock, manifest, toolchain)
            with tarfile.open(archive) as result:
                self.assertEqual(
                    result.extractfile("yamaa/rust/Cargo.toml").read(), manifest
                )
                self.assertEqual(
                    result.extractfile("yamaa/rust/Cargo.lock").read(), lock
                )
                self.assertEqual(
                    result.extractfile("yamaa/rust/rust-toolchain.toml").read(),
                    toolchain,
                )
                self.assertEqual(
                    result.extractfile("yamaa/pyproject.toml").read(), values[2][1]
                )
                self.assertEqual(
                    result.extractfile("yamaa/rust/crates/r/src/lib.rs").read(),
                    (workspace / "crates/r/src/lib.rs").read_bytes(),
                )
                self.assertEqual(len(result.getnames()), len(values) + 3)
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
            (root / "rust-toolchain.toml").write_bytes(b"toolchain")
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

    def test_external_source_symlink_refuses_without_replacing_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest = b'[workspace]\nmembers = ["crates/r"]\n'
            (root / "Cargo.toml").write_bytes(manifest)
            (root / "rust-toolchain.toml").write_bytes(b"toolchain")
            (root / "Cargo.lock").write_bytes(b"canonical")
            member = root / "crates/r"
            (member / "src").mkdir(parents=True)
            (member / "Cargo.toml").write_bytes(b"member")
            (root / "private.rs").write_bytes(b"outside member")
            archive = root / "yamaa.tar.gz"
            write_archive(
                archive,
                [
                    ("yamaa/rust/Cargo.toml", b"pruned"),
                    ("yamaa/rust/Cargo.lock", b"canonical"),
                ],
            )
            original = archive.read_bytes()
            try:
                (member / "src/lib.rs").symlink_to(root / "private.rs")
            except OSError as error:
                self.skipTest(f"source symlink witness is unavailable: {error}")
            with self.assertRaisesRegex(ValueError, "symlinks"):
                backend.preserve_workspace(archive, root)
            self.assertEqual(archive.read_bytes(), original)
            self.assertEqual(list(root.glob("*.tar.gz")), [archive])

    def test_source_directory_symlink_refuses(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            member = root / "crates/r"
            (member / "src").mkdir(parents=True)
            (member / "Cargo.toml").write_bytes(b"member")
            (root / "outside").mkdir()
            (root / "outside/private.rs").write_bytes(b"outside member")
            try:
                (member / "src/linked").symlink_to(
                    root / "outside", target_is_directory=True
                )
            except OSError as error:
                self.skipTest(f"directory symlink witness is unavailable: {error}")
            with self.assertRaisesRegex(ValueError, "symlinks"):
                list(backend.member_sources(member))

    def test_invalid_archive_roots_refuse_without_replacement(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_bytes(b"[workspace]\nmembers = []\n")
            (root / "rust-toolchain.toml").write_bytes(b"toolchain")
            (root / "Cargo.lock").write_bytes(b"canonical")
            archive = root / "yamaa.tar.gz"
            for package in (
                "/yamaa",
                "../yamaa",
                "./yamaa",
                "yamaa/..",
                "C:",
                "C:\\yamaa",
            ):
                with self.subTest(package=package):
                    write_archive(
                        archive,
                        [
                            (f"{package}/rust/Cargo.toml", b"pruned"),
                            (f"{package}/rust/Cargo.lock", b"canonical"),
                        ],
                    )
                    original = archive.read_bytes()
                    with self.assertRaisesRegex(ValueError, "invalid package root"):
                        backend.preserve_workspace(archive, root)
                    self.assertEqual(archive.read_bytes(), original)

    def test_contradictory_archive_refuses_without_replacing_original(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text("[workspace]\nmembers = []\n")
            (root / "rust-toolchain.toml").write_bytes(b"toolchain")
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
