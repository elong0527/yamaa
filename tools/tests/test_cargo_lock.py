"""Exercise real Cargo refusal and the engine bundles that both source archives carry."""

import io
import json
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path

from tools.packaging import check_cargo_lock as guard
from tools.staging import engine_bundle


def archive(path, entries, mode="w:gz"):
    """Write (name, payload) entries; a str payload is a link's target."""
    with tarfile.open(path, mode) as output:
        for name, value in entries:
            member = tarfile.TarInfo(name)
            if isinstance(value, bytes):
                member.size = len(value)
                output.addfile(member, io.BytesIO(value))
            else:
                member.type = tarfile.SYMTYPE
                member.linkname = value
                output.addfile(member)


def packed(entries):
    """The bytes of an xz tar of (name, payload) entries, as archive() writes."""
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "vendor.tar.xz"
        archive(path, entries, "w:xz")
        return path.read_bytes()


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
    """Each host's source archive bundle, against a small checkout."""

    ENGINE = [
        ("yamaa", "0.5.0", False, ["yamaa-adapters", "yamaa-core"]),
        ("yamaa-adapters", "0.5.0", False, ["parser", "yamaa-core"]),
        ("yamaa-core", "0.5.0", False, []),
        ("parser", "1.0.0", True, []),
    ]
    BINDINGS = {
        "python": [
            ("yamaa-python", "0.5.0", False, ["pyo3", "yamaa"]),
            ("pyo3", "0.27.2", True, []),
        ],
        "r": [
            ("extendr", "0.9.0", True, []),
            ("yamaa-r", "0.5.0", False, ["extendr", "yamaa"]),
        ],
    }
    # Each host's vendored crates, by directory, with their files.
    CRATES = {
        "python": {
            "parser-1.0.0": {"Cargo.toml": b""},
            "pyo3-0.27.2": {"Cargo.toml": b"", "src/lib.rs": b"//\n"},
        },
        "r": {
            "parser-1.0.0": {"Cargo.toml": b""},
            "extendr-0.9.0": {"Cargo.toml": b"", "src/lib.rs": b"//\n"},
        },
    }

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
        self.canonical = lock_text(
            self.ENGINE + self.BINDINGS["python"] + self.BINDINGS["r"]
        )
        (self.workspace / "Cargo.lock").write_text(self.canonical)
        self.path = self.root / "source.tar.gz"

    def vendored(self, crates, unlisted=()):
        """The vendor/ files of crates, each with a checksum record that lists
        its files but the `unlisted` ones."""
        files = {}
        for crate, contents in crates.items():
            listed = [name for name in contents if f"{crate}/{name}" not in unlisted]
            record = {"files": dict.fromkeys(listed, "recorded"), "package": "p"}
            files[f"vendor/{crate}/.cargo-checksum.json"] = json.dumps(record).encode()
            for name, payload in contents.items():
                files[f"vendor/{crate}/{name}"] = payload
        return files

    def entries(self, host, vendored=None, links=()):
        """A host's source archive files: its bundle and a package file.

        `links` are (name, target) links among the vendored crates."""
        layout = guard.LAYOUTS[host]
        entries = dict(guard.bundle_files(host, self.workspace))
        entries[layout.workspace + "Cargo.lock"] = lock_text(
            self.ENGINE + self.BINDINGS[host]
        ).encode()
        if vendored is None:
            vendored = self.vendored(self.CRATES[host])
        if layout.packed:
            entries[layout.packed] = packed([*vendored.items(), *links])
            entries["DESCRIPTION"] = b"Package: yamaa\n"
        else:
            self.assertEqual(links, ())
            entries.update(vendored)
            entries["pyproject.toml"] = b"[project]\n"
        return entries

    def check(self, entries, links=()):
        archive(
            self.path,
            [(f"yamaa/{name}", payload) for name, payload in entries.items()]
            + [(f"yamaa/{name}", target) for name, target in links],
        )
        return guard.check_bundle_archive(self.path, self.workspace)

    def test_each_host_bundle_from_the_checkout_is_admitted(self):
        for host in ("python", "r"):
            with self.subTest(host=host):
                entries = self.entries(host)
                found, kept = self.check(entries)
                self.assertEqual(found, host)
                self.assertEqual(
                    sorted((name, version) for name, version, _ in kept),
                    sorted(
                        (name, version)
                        for name, version, _, _ in self.ENGINE + self.BINDINGS[host]
                    ),
                )
                layout = guard.LAYOUTS[host]
                self.assertEqual(
                    entries[layout.workspace + "Cargo.toml"],
                    engine_bundle.workspace_manifest(self.workspace).encode(),
                )
                self.assertEqual(
                    entries[layout.config], engine_bundle.CARGO_CONFIG.encode()
                )

    def test_no_toolchain_file_anywhere(self):
        for host, name in (
            ("python", "rust-toolchain.toml"),
            ("python", "rust/rust-toolchain.toml"),
            ("python", "vendor/pyo3-0.27.2/rust-toolchain"),
            ("python", "src/yamaa/rust-toolchain.toml"),
            ("r", "src/rust/rust-toolchain.toml"),
            ("r", "src/rust/crates/yamaa-r/rust-toolchain"),
            ("r", "inst/rust-toolchain.toml"),
        ):
            with self.subTest(host=host, name=name):
                with self.assertRaisesRegex(ValueError, f"carries {name}$"):
                    self.check(dict(self.entries(host), **{name: b""}))
        # In the R source package, nor inside its compressed crates, listed
        # in their checksum records or not.
        for crate, unlisted in (
            ({"rust-toolchain.toml": b"[toolchain]\n"}, ()),
            (
                {"tests/rust-toolchain": b"nightly\n"},
                ("extendr-0.9.0/tests/rust-toolchain",),
            ),
        ):
            crates = {
                **self.CRATES["r"],
                "extendr-0.9.0": {**self.CRATES["r"]["extendr-0.9.0"], **crate},
            }
            name = f"vendor/extendr-0.9.0/{next(iter(crate))}"
            with self.subTest(name=name):
                with self.assertRaisesRegex(
                    ValueError, f"src/rust/vendor.tar.xz carries {name}$"
                ):
                    self.check(self.entries("r", self.vendored(crates, unlisted)))

    def test_python_files_beside_or_changed_from_the_bundle_refuse(self):
        entries = self.entries("python")
        for name, payload, message in (
            (
                "rust/crates/yamaa-core/tests/case.rs",
                b"//\n",
                "checkout at rust/crates",
            ),
            ("rust/crates/yamaa/public-api.txt", b"api\n", "checkout at rust/crates"),
            ("rust/crates/yamaa-core/src/lib.rs", b"changed\n", "differs from the"),
            (".cargo/config.toml", b"[net]\noffline = false\n", "differs from the"),
            (
                "rust/Cargo.lock",
                self.canonical.encode(),
                "extendr 0.9.0 is outside the dependency",
            ),
            ("vendor/pyo3-0.27.2/src/extra.rs", b"", "files its .cargo-checksum"),
            ("vendor/extendr-0.9.0/Cargo.toml", b"", "vendored crate extendr-0.9.0"),
        ):
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, message):
                self.check(dict(entries, **{name: payload}))
        for missing, message in (
            ("rust/crates/yamaa-python/src/lib.rs", "checkout at rust/crates"),
            ("vendor/pyo3-0.27.2/src/lib.rs", "files its .cargo-checksum"),
            ("vendor/pyo3-0.27.2/.cargo-checksum.json", "pyo3-0.27.2 has no"),
        ):
            with self.subTest(missing=missing):
                with self.assertRaisesRegex(ValueError, message):
                    self.check({k: v for k, v in entries.items() if k != missing})
        without = {k: v for k, v in entries.items() if "pyo3" not in k}
        with self.assertRaisesRegex(ValueError, "vendored crate pyo3-0.27.2"):
            self.check(without)
        with self.assertRaisesRegex(ValueError, "must not contain links"):
            self.check(entries, [("vendor/link", "pyo3-0.27.2")])
        for name in ("rust/../escape.rs", "rust//crates.rs", "rust\\crates.rs"):
            with self.subTest(name=name):
                with self.assertRaisesRegex(ValueError, "invalid member name"):
                    self.check(dict(entries, **{name: b""}))

    def test_r_files_beside_or_changed_from_the_bundle_refuse(self):
        entries = self.entries("r")
        for name, payload, message in (
            (
                "src/rust/crates/yamaa-core/tests/case.rs",
                b"//\n",
                "checkout at src/rust/crates/yamaa-core/tests",
            ),
            (
                "src/rust/crates/yamaa-python/src/lib.rs",
                b"//\n",
                "checkout at src/rust/crates/yamaa-python",
            ),
            (
                "src/rust/target/release/libyamaa_r.a",
                b"",
                "checkout at src/rust/target",
            ),
            ("src/rust/crates/yamaa-r/src/lib.rs", b"changed\n", "differs from the"),
            ("src/rust/vendor-config.toml", b"[source]\n", "differs from the"),
            (
                "src/rust/Cargo.lock",
                self.canonical.encode(),
                "pyo3 0.27.2 is outside the dependency",
            ),
        ):
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, message):
                self.check(dict(entries, **{name: payload}))
        for missing, message in (
            ("src/rust/crates/yamaa-r/src/lib.rs", "checkout at src/rust/crates"),
            ("src/rust/vendor-config.toml", "checkout at src/rust/vendor-config"),
            ("src/rust/vendor.tar.xz", "the bundle is missing src/rust/vendor.tar.xz"),
        ):
            with self.subTest(missing=missing):
                with self.assertRaisesRegex(ValueError, message):
                    self.check({k: v for k, v in entries.items() if k != missing})
        # Files beside the bundle and outside src/rust are the R package's own.
        self.assertEqual(self.check(dict(entries, **{"src/entrypoint.c": b""}))[0], "r")

    def test_r_compressed_crates_follow_the_lock_and_their_records(self):
        crates = self.CRATES["r"]
        extendr = crates["extendr-0.9.0"]
        for vendored, links, message in (
            (
                self.vendored({**crates, "pyo3-0.27.2": {"Cargo.toml": b""}}),
                (),
                "vendored crate pyo3-0.27.2 differs from the lock's crates",
            ),
            (
                self.vendored({"parser-1.0.0": crates["parser-1.0.0"]}),
                (),
                "vendored crate extendr-0.9.0 differs from the lock's crates",
            ),
            (
                self.vendored(
                    {**crates, "extendr-0.9.0": {**extendr, "src/extra.rs": b""}},
                    ("extendr-0.9.0/src/extra.rs",),
                ),
                (),
                "extendr-0.9.0 differs from the files its .cargo-checksum.json",
            ),
            (
                {
                    k: v
                    for k, v in self.vendored(crates).items()
                    if k != "vendor/extendr-0.9.0/.cargo-checksum.json"
                },
                (),
                "extendr-0.9.0 has no .cargo-checksum.json",
            ),
            (self.vendored(crates), [("vendor/link", "extendr-0.9.0")], "links"),
            (
                {**self.vendored(crates), "other/file.rs": b""},
                (),
                "members must sit below vendor/",
            ),
            (
                {**self.vendored(crates), "vendor/../escape.rs": b""},
                (),
                "invalid member name",
            ),
        ):
            with self.subTest(message=message):
                with self.assertRaisesRegex(ValueError, message):
                    self.check(self.entries("r", vendored, links))


if __name__ == "__main__":
    unittest.main()
