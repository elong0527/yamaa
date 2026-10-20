"""Exercise real Cargo refusal and the locks that both source archives carry."""

import io
import json
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path

from tools.packaging import check_cargo_lock as guard
from tools.staging import engine_bundle


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

    def test_absolute_and_dot_rooted_locks_refuse_even_with_matching_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "yamaa.tar.gz"
            for location in (
                "/rust/Cargo.lock",
                "../rust/Cargo.lock",
                "./yamaa/rust/Cargo.lock",
                "yamaa/./rust/Cargo.lock",
                "yamaa//rust/Cargo.lock",
                "yamaa/src/../rust/Cargo.lock",
                "C:/rust/Cargo.lock",
                "C:\\yamaa/rust/Cargo.lock",
            ):
                with self.subTest(location=location):
                    archive(
                        path,
                        [
                            (location, b"lock"),
                            (
                                location.removesuffix("Cargo.lock") + "Cargo.toml",
                                b"manifest",
                            ),
                        ],
                    )
                    with self.assertRaisesRegex(ValueError, "unexpected"):
                        guard.check_archive(path, b"lock", b"manifest")

    def test_source_packages_preserve_the_declared_rust_toolchain(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "yamaa.tar.gz"
            entries = [
                ("yamaa/rust/Cargo.lock", b"lock"),
                ("yamaa/rust/Cargo.toml", b"manifest"),
            ]
            for declarations, message in (
                ([], "one regular"),
                ([("yamaa/rust/rust-toolchain.toml", b"other")], "toolchain differs"),
                ([("yamaa/rust/rust-toolchain.toml", "elsewhere")], "one regular"),
                ([("yamaa/rust/rust-toolchain.toml", b"toolchain")] * 2, "one regular"),
            ):
                with self.subTest(declarations=declarations):
                    archive(path, entries + declarations)
                    with self.assertRaisesRegex(ValueError, message):
                        guard.check_archive(path, b"lock", b"manifest", b"toolchain")
            archive(path, entries + [("yamaa/rust/rust-toolchain.toml", b"toolchain")])
            guard.check_archive(path, b"lock", b"manifest", b"toolchain")

    def test_the_r_package_carries_the_vendored_parser_unchanged(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "yamaa.tar.gz"
            entries = [
                ("yamaa/src/rust/Cargo.lock", b"lock"),
                ("yamaa/src/rust/Cargo.toml", b"manifest"),
            ]
            source = ("vendor/witness/src/lib.rs", b"pub fn witness() {}\n")
            name = "yamaa/src/rust/" + source[0]
            for extra, message in (
                ([], "one regular vendored crate source"),
                ([(name, b"changed parser source")], "differs from canonical bytes"),
                (
                    [(name, source[1]), (name.replace("lib.rs", "build.rs"), b"")],
                    "unexpected vendored crate file",
                ),
                (
                    [(name, source[1]), ("yamaa/src/rust/vendor/other", "witness")],
                    "must not contain links",
                ),
            ):
                with self.subTest(message=message):
                    archive(path, entries + extra)
                    with self.assertRaisesRegex(ValueError, message):
                        guard.check_archive(path, b"lock", b"manifest", None, [source])
            archive(path, entries + [(name, source[1])])
            guard.check_archive(path, b"lock", b"manifest", None, [source])
            self.assertTrue(guard.is_r_source(path))


REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"


def lock_text(packages):
    lines = ["version = 4"]
    for name, version, registry, dependencies in packages:
        lines += ["", "[[package]]", f'name = "{name}"', f'version = "{version}"']
        if registry:
            lines += [f'source = "{REGISTRY}"', f'checksum = "recorded {name}"']
        if dependencies:
            lines.append(
                "dependencies = [" + ", ".join(f'"{d}"' for d in dependencies) + "]"
            )
    return "\n".join(lines) + "\n"


class BundleArchiveTests(unittest.TestCase):
    """The Python source archive's bundle, against a small checkout."""

    PYTHON = [
        ("yamaa", "0.5.0", False, ["yamaa-adapters", "yamaa-core"]),
        ("yamaa-adapters", "0.5.0", False, ["parser", "yamaa-core"]),
        ("yamaa-core", "0.5.0", False, []),
        ("yamaa-python", "0.5.0", False, ["pyo3", "yamaa"]),
        ("parser", "1.0.0", True, []),
        ("pyo3", "0.27.2", True, []),
    ]
    R = [
        ("extendr", "0.9.0", True, []),
        ("yamaa-r", "0.5.0", False, ["extendr", "yamaa"]),
    ]

    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.workspace = self.root / "rust"
        crates = ("yamaa-core", "yamaa-adapters", "yamaa", "yamaa-python", "yamaa-r")
        for crate in crates:
            (self.workspace / "crates" / crate / "src").mkdir(parents=True)
            (self.workspace / "crates" / crate / "Cargo.toml").write_text(
                f'[package]\nname = "{crate}"\n'
            )
            (self.workspace / "crates" / crate / "src/lib.rs").write_text("//\n")
        listed = ", ".join(f'"crates/{crate}"' for crate in crates)
        (self.workspace / "Cargo.toml").write_text(
            f'[workspace]\nmembers = [{listed}]\nresolver = "2"\n'
        )
        (self.workspace / "Cargo.lock").write_text(lock_text(self.PYTHON + self.R))
        self.entries = dict(guard.bundle_files(self.workspace))
        self.entries["rust/Cargo.lock"] = lock_text(self.PYTHON).encode()
        self.entries["pyproject.toml"] = b"[project]\n"
        for crate, files in (
            ("parser-1.0.0", {"Cargo.toml": b"", "rust-toolchain.toml": b""}),
            ("pyo3-0.27.2", {"Cargo.toml": b"", "src/lib.rs": b"//\n"}),
        ):
            record = {"files": dict.fromkeys(files, "recorded"), "package": "recorded"}
            files[".cargo-checksum.json"] = json.dumps(record).encode()
            for name, payload in files.items():
                self.entries[f"vendor/{crate}/{name}"] = payload
        self.path = self.root / "yamaa-0.5.0.tar.gz"

    def check(self, entries, links=()):
        archive(
            self.path,
            [(f"yamaa-0.5.0/{name}", payload) for name, payload in entries.items()]
            + [(f"yamaa-0.5.0/{name}", target) for name, target in links],
        )
        self.assertFalse(guard.is_r_source(self.path))
        return guard.check_bundle_archive(self.path, self.workspace)

    def test_the_bundle_from_the_checkout_is_admitted(self):
        kept = self.check(self.entries)
        self.assertEqual(len(kept), len(self.PYTHON))
        self.assertIn(".cargo/config.toml", self.entries)
        self.assertEqual(
            self.entries["rust/Cargo.toml"],
            engine_bundle.workspace_manifest(self.workspace).encode(),
        )

    def test_files_beside_or_changed_from_the_bundle_refuse(self):
        lock = (self.workspace / "Cargo.lock").read_bytes()
        for name, payload, message in (
            (
                "rust/crates/yamaa-core/tests/case.rs",
                b"//\n",
                "checkout at rust/crates",
            ),
            ("rust/crates/yamaa/public-api.txt", b"api\n", "checkout at rust/crates"),
            ("rust/rust-toolchain.toml", b"", "checkout at rust/rust-toolchain"),
            ("rust-toolchain.toml", b"", "carries rust-toolchain.toml"),
            ("rust/crates/yamaa-core/src/lib.rs", b"changed\n", "differs from the"),
            (".cargo/config.toml", b"[net]\noffline = false\n", "differs from the"),
            ("rust/Cargo.lock", lock, "extendr 0.9.0 is outside the dependency"),
            ("vendor/pyo3-0.27.2/src/extra.rs", b"", "files its .cargo-checksum"),
            ("vendor/extendr-0.9.0/Cargo.toml", b"", "vendored crate extendr-0.9.0"),
        ):
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, message):
                self.check(dict(self.entries, **{name: payload}))
        for missing, message in (
            ("rust/crates/yamaa-python/src/lib.rs", "checkout at rust/crates"),
            ("vendor/pyo3-0.27.2/src/lib.rs", "files its .cargo-checksum"),
        ):
            entries = dict(self.entries)
            del entries[missing]
            with self.subTest(missing=missing):
                with self.assertRaisesRegex(ValueError, message):
                    self.check(entries)
        entries = {k: v for k, v in self.entries.items() if "pyo3" not in k}
        with self.assertRaisesRegex(ValueError, "vendored crate pyo3-0.27.2"):
            self.check(entries)
        with self.assertRaisesRegex(ValueError, "must not contain links"):
            self.check(self.entries, [("vendor/link", "pyo3-0.27.2")])
        for name in ("rust/../escape.rs", "rust//crates.rs", "rust\\crates.rs"):
            with self.subTest(name=name):
                with self.assertRaisesRegex(ValueError, "invalid member name"):
                    self.check(dict(self.entries, **{name: b""}))


if __name__ == "__main__":
    unittest.main()
