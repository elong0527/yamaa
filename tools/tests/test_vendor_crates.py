"""The vendored crate archive: reproducible, binary-free and standalone."""

import ast
import io
import json
import os
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest import mock

from tools.staging import stage_r_package as staging
from tools.staging import vendor_crates

CHECKSUMS = {"$comment": "x", "files": {}, "package": "p"}


def crate(directory, name, version, files):
    """Write a vendored crate with its manifest and checksum record."""
    root = directory / f"{name}-{version}"
    root.mkdir(parents=True)
    (root / "Cargo.toml").write_text(
        f'[package]\nname = "{name}"\nversion = "{version}"\n'
    )
    record = dict(CHECKSUMS, files={})
    for relative, payload in files.items():
        (root / relative).parent.mkdir(parents=True, exist_ok=True)
        (root / relative).write_bytes(payload)
        record["files"][relative] = "recorded"
    (root / ".cargo-checksum.json").write_text(json.dumps(record))
    return root


class ArchiveTests(unittest.TestCase):
    def test_archive_bytes_depend_only_on_the_files(self):
        with tempfile.TemporaryDirectory() as scratch:
            scratch = Path(scratch)
            archives = []
            for index, mode in enumerate((0o644, 0o755)):
                directory = scratch / f"tree{index}"
                root = crate(directory, "zeta", "1.0.0", {"src/lib.rs": b"//\n"})
                crate(directory, "alpha", "0.1.0", {"build.rs": b"fn main() {}\n"})
                os.chmod(root / "src/lib.rs", mode)
                os.utime(root / "src/lib.rs", (index * 1000, index * 1000))
                archive = scratch / f"vendor{index}.tar.xz"
                vendor_crates.write_archive(directory, archive)
                archives.append(archive.read_bytes())
            self.assertEqual(archives[0], archives[1])
            with tarfile.open(fileobj=io.BytesIO(archives[0]), mode="r:xz") as tar:
                members = tar.getmembers()
            names = [member.name for member in members]
            self.assertEqual(names[0], "vendor")
            self.assertEqual(names, sorted(names))
            self.assertIn("vendor/zeta-1.0.0/src/lib.rs", names)
            for member in members:
                self.assertEqual(member.mtime, vendor_crates.MTIME)
                self.assertEqual((member.uid, member.gid), (0, 0))
                self.assertEqual((member.uname, member.gname), ("", ""))
                self.assertEqual(member.mode, 0o755 if member.isdir() else 0o644)

    def test_archive_refuses_links_and_an_existing_archive(self):
        with tempfile.TemporaryDirectory() as scratch:
            scratch = Path(scratch)
            directory = scratch / "tree"
            root = crate(directory, "alpha", "0.1.0", {"src/lib.rs": b"//\n"})
            archive = scratch / "vendor.tar.xz"
            archive.write_bytes(b"prior")
            with self.assertRaises(FileExistsError):
                vendor_crates.write_archive(directory, archive)
            self.assertEqual(archive.read_bytes(), b"prior")
            (root / "link.rs").symlink_to(root / "src/lib.rs")
            with self.assertRaisesRegex(ValueError, "only files and directories"):
                vendor_crates.write_archive(directory, scratch / "other.tar.xz")

    def test_config_replaces_crates_io_with_the_archive_directory(self):
        config = tomllib.loads(vendor_crates.CONFIG)["source"]
        self.assertEqual(config["crates-io"], {"replace-with": "vendored-sources"})
        self.assertEqual(
            config["vendored-sources"], {"directory": vendor_crates.DIRECTORY}
        )


class BinaryTests(unittest.TestCase):
    def test_binaries_are_recognized_by_header_or_name(self):
        pe = bytearray(128)
        pe[:2], pe[0x3C:0x40], pe[64:68] = b"MZ", (64).to_bytes(4, "little"), b"PE\0\0"
        cases = {
            "elf": (b"\x7fELF\x02\x01", True),
            "macho": (b"\xcf\xfa\xed\xfe\x07", True),
            "archive": (b"!<arch>\nobject", True),
            "module": (b"\x00asm\x01\x00\x00\x00", True),
            "program": (bytes(pe), True),
            "object.o": (b"text", True),
            "library.so.1": (b"text", True),
            "notes.md": (b"MZ is not a PE header\n", False),
            "lib.rs": (b"fn main() {}\n", False),
            "data.bin": (b"\x00\x01\x02", False),
        }
        with tempfile.TemporaryDirectory() as scratch:
            for name, (payload, expected) in cases.items():
                path = Path(scratch) / name
                path.write_bytes(payload)
                with self.subTest(name=name):
                    self.assertIs(vendor_crates.is_binary(path), expected)

    def test_listed_binaries_leave_the_crate_and_its_checksum_record(self):
        files = {
            "src/lib.rs": b"//\n",
            "src/rt/runtime.o": b"\x00asm\x01",
            "src/rt/runtime.c": b"int x;\n",
        }
        with (
            tempfile.TemporaryDirectory() as scratch,
            mock.patch.dict(vendor_crates.PREBUILT, {"wasm": ("src/rt/runtime.o",)}),
        ):
            root = crate(Path(scratch), "wasm", "1.0.0", files)
            removed = vendor_crates.leave_out_prebuilt(root, "wasm", root.name)
            self.assertEqual(removed, ["src/rt/runtime.o"])
            self.assertFalse((root / "src/rt/runtime.o").exists())
            self.assertTrue((root / "src/rt/runtime.c").is_file())
            record = json.loads((root / ".cargo-checksum.json").read_text())
            self.assertEqual(
                sorted(record["files"]), ["src/lib.rs", "src/rt/runtime.c"]
            )
            self.assertEqual(record["package"], "p")

    def test_unreviewed_or_vanished_binaries_stop_vendoring(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = crate(Path(scratch), "tool", "1.0.0", {"bin/tool": b"\x7fELF\x02"})
            with self.assertRaisesRegex(ValueError, "prebuilt binary bin/tool"):
                vendor_crates.leave_out_prebuilt(root, "tool", root.name)
            self.assertTrue((root / "bin/tool").is_file())
            (root / "bin/tool").unlink()
            with (
                mock.patch.dict(vendor_crates.PREBUILT, {"tool": ("bin/gone.o",)}),
                self.assertRaisesRegex(ValueError, "no longer ships"),
            ):
                vendor_crates.leave_out_prebuilt(root, "tool", root.name)


class VendorTests(unittest.TestCase):
    def vendor(self, populate):
        """Run vendor() with `cargo vendor` replaced by populate(directory)."""

        def run(command, cwd, stdout, check):
            self.assertEqual(
                command[:4], ["cargo", "vendor", "--locked", "--versioned-dirs"]
            )
            populate(Path(command[-1]))

        with tempfile.TemporaryDirectory() as scratch:
            with mock.patch.object(vendor_crates.subprocess, "run", run):
                return vendor_crates.vendor(Path(scratch), Path(scratch) / "vendor")

    def test_vendored_crates_are_named_by_their_manifests(self):
        def populate(directory):
            crate(directory, "zeta", "1.0.0+extra", {"src/lib.rs": b"//\n"})
            crate(
                directory,
                "wit-bindgen",
                "0.1.0",
                dict.fromkeys(vendor_crates.PREBUILT["wit-bindgen"], b"\x00asm\x01"),
            )

        crates = self.vendor(populate)
        self.assertEqual(
            crates,
            [
                ("wit-bindgen", "0.1.0", sorted(vendor_crates.PREBUILT["wit-bindgen"])),
                ("zeta", "1.0.0+extra", []),
            ],
        )

    def test_unexpected_vendor_output_is_refused(self):
        def misnamed(directory):
            root = crate(directory, "alpha", "1.0.0", {})
            root.rename(directory / "beta-1.0.0")

        def unrecorded(directory):
            crate(directory, "alpha", "1.0.0", {})
            (directory / "alpha-1.0.0/.cargo-checksum.json").unlink()

        def stale_table(directory):
            crate(directory, "alpha", "1.0.0", {})

        for populate, message in (
            (misnamed, "declares alpha 1.0.0"),
            (unrecorded, "has no .cargo-checksum.json"),
            (stale_table, "PREBUILT lists wit-bindgen"),
        ):
            with self.subTest(case=populate.__name__):
                with self.assertRaisesRegex(ValueError, message):
                    self.vendor(populate)

    def test_the_script_runs_without_the_repository(self):
        # The R source package carries this file alone as tools/vendor_crates.py.
        source = Path(vendor_crates.__file__)
        tree = ast.parse(source.read_text(encoding="utf-8"))
        imported = {
            alias.name.split(".")[0]
            for node in ast.walk(tree)
            if isinstance(node, ast.Import)
            for alias in node.names
        } | {
            node.module.split(".")[0]
            for node in ast.walk(tree)
            if isinstance(node, ast.ImportFrom)
        }
        self.assertLessEqual(imported, set(sys.stdlib_module_names))
        with tempfile.TemporaryDirectory() as scratch:
            copy = Path(scratch) / "vendor_crates.py"
            copy.write_bytes(source.read_bytes())
            result = subprocess.run(
                [sys.executable, "-I", str(copy), "--help"],
                cwd=scratch,
                capture_output=True,
                text=True,
                check=True,
            )
        self.assertIn("workspace", result.stdout)


class AuthorsTests(unittest.TestCase):
    def metadata(self):
        def package(name, version, source, **fields):
            return {
                "id": f"{name} {version}",
                "name": name,
                "version": version,
                "source": source,
                "authors": fields.get("authors", []),
                "license": fields.get("license", "MIT"),
                "repository": fields.get("repository"),
                "manifest_path": fields.get("manifest_path", "/x/Cargo.toml"),
            }

        registry = staging.notices.CRATES_IO
        parser = staging.WORKSPACE / "vendor/saphyr-parser/Cargo.toml"
        return {
            "workspace_members": ["yamaa-r 0.5.0"],
            "packages": [
                package("yamaa-r", "0.5.0", None),
                package("zeta", "1.10.0", registry, authors=["Z <z@example.org>"]),
                package("zeta", "1.9.0", registry, repository="https://example.org/z"),
                package("alpha", "0.1.0", registry, authors=["A", "B"]),
                package("parser", "0.2.1", None, manifest_path=str(parser)),
            ],
        }

    def test_every_bundled_crate_is_named_with_its_declared_facts(self):
        crates = [
            ("alpha", "0.1.0", ["src/rt/runtime.o"]),
            ("zeta", "1.9.0", []),
            ("zeta", "1.10.0", []),
        ]
        listed = {
            ("alpha", "0.1.0"),
            ("zeta", "1.9.0"),
            ("zeta", "1.10.0"),
            ("parser", "0.2.1"),
        }
        text = staging.authors_text(self.metadata(), crates, listed)
        entries = text.split("\nCrate: ")[1:]
        self.assertEqual(
            [entry.splitlines()[0] for entry in entries],
            ["alpha 0.1.0", "parser 0.2.1", "zeta 1.9.0", "zeta 1.10.0"],
        )
        self.assertIn("    Authors: A, B\n", entries[0])
        self.assertIn("    Left out: src/rt/runtime.o\n", entries[0])
        self.assertIn("    Directory: src/rust/vendor/saphyr-parser\n", entries[1])
        self.assertIn("    Authors: none declared\n", entries[2])
        self.assertIn("    Repository: https://example.org/z\n", entries[2])
        self.assertIn("    Repository: none declared\n", entries[3])
        self.assertNotIn("yamaa-r", text)
        self.assertTrue(text.endswith("\n"))

    def test_unvendored_or_unlicensed_crates_stop_staging(self):
        crates = [("alpha", "0.1.0", []), ("zeta", "1.9.0", []), ("zeta", "1.10.0", [])]
        listed = {
            ("alpha", "0.1.0"),
            ("zeta", "1.9.0"),
            ("zeta", "1.10.0"),
            ("parser", "0.2.1"),
        }
        with self.assertRaisesRegex(
            ValueError, "zeta 1.10.0: the vendored crates differ"
        ):
            staging.authors_text(self.metadata(), crates[:2], listed)
        with self.assertRaisesRegex(ValueError, "parser 0.2.1: LICENSE-THIRD-PARTY"):
            staging.authors_text(
                self.metadata(), crates, listed - {("parser", "0.2.1")}
            )


if __name__ == "__main__":
    unittest.main()
