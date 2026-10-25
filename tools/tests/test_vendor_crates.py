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

    def test_a_binary_stops_vendoring_a_crate_that_a_target_compiles(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = crate(Path(scratch), "tool", "1.0.0", {"bin/tool": b"\x7fELF\x02"})
            with self.assertRaisesRegex(ValueError, "prebuilt binary bin/tool"):
                vendor_crates.binaries(root, root.name)
            self.assertTrue((root / "bin/tool").is_file())
            (root / "bin/tool").unlink()
            vendor_crates.binaries(root, root.name)


class AlternativeLicenseTests(unittest.TestCase):
    """The packages carry no text of a license that yamaa does not rely on."""

    def test_listed_texts_leave_the_crate_and_its_checksum_record(self):
        files = {
            "src/lib.rs": b"//\n",
            "zstd/COPYING": b"the other license\n",
            "zstd/LICENSE": b"BSD License\n",
        }
        with tempfile.TemporaryDirectory() as scratch:
            root = crate(Path(scratch), "zstd-sys", "2.1.2", files)
            found = vendor_crates.alternative_licenses(root, "zstd-sys", root.name)
            self.assertEqual(found, ["zstd/COPYING"])
            vendor_crates.leave_out(root, found, root.name)
            self.assertFalse((root / "zstd/COPYING").exists())
            self.assertTrue((root / "zstd/LICENSE").is_file())
            record = json.loads((root / ".cargo-checksum.json").read_text())
            self.assertEqual(sorted(record["files"]), ["src/lib.rs", "zstd/LICENSE"])
            self.assertEqual(record["package"], "p")
            # Other crates ship none.
            self.assertEqual(
                vendor_crates.alternative_licenses(root, "zstd", root.name), []
            )

    def test_a_vanished_text_stops_vendoring(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = crate(Path(scratch), "zstd-sys", "2.1.3", {"src/lib.rs": b"//\n"})
            with self.assertRaisesRegex(
                ValueError, "no longer ships the listed license text zstd/COPYING"
            ):
                vendor_crates.alternative_licenses(root, "zstd-sys", root.name)


class DevelopmentFilesTests(unittest.TestCase):
    def test_only_audited_versioned_development_files_leave_the_crate_and_record(self):
        files = {
            "Cargo.toml": (
                b'[package]\nname="plain"\nversion="1.0.0"\n'
                b'readme="examples/README.md"\nlicense-file="tests/LEGAL.txt"\n'
                b'build="examples/build.rs"\n[lib]\npath="examples/lib.rs"\n'
                b'[[bin]]\nname="tool"\npath="examples/tool.rs"\n'
            ),
            "src/lib.rs": b"// production\n",
            "LICENSE-MIT": b"Copyright\n",
            "examples/README.md": b"embedded documentation\n",
            "examples/build.rs": b"fn main() {}\n",
            "examples/lib.rs": b"// production library\n",
            "examples/tool.rs": b"fn main() {}\n",
            "tests/LEGAL.txt": b"the declared license\n",
            "tests/NOTICE": b"retained notice\n",
            "tests/LICENSE-MIT": b"retained license\n",
            ".github/workflows/check.yml": b"# development\n",
            "benches/bench.rs": b"// benchmark\n",
            "examples/demo.rs": b"// standalone example\n",
            "tests/case.rs": b"// unit test\n",
        }
        policy = {"plain-1.0.0": [".github", "benches", "examples", "tests"]}
        with tempfile.TemporaryDirectory() as scratch:
            root = crate(Path(scratch), "plain", "1.0.0", files)
            left_out = vendor_crates.development_files(root, policy)
            self.assertEqual(
                left_out,
                [
                    ".github/workflows/check.yml",
                    "benches/bench.rs",
                    "examples/demo.rs",
                    "tests/case.rs",
                ],
            )
            vendor_crates.leave_out(root, left_out, root.name)
            remaining = files_of(root)
            record = json.loads(remaining.pop(".cargo-checksum.json"))
            self.assertEqual(
                remaining,
                {name: data for name, data in files.items() if name not in left_out},
            )
            self.assertEqual(record["files"], {name: "recorded" for name in remaining})
            self.assertEqual(record["package"], "p")
            # An upgraded version has not been audited and keeps everything.
            other = crate(Path(scratch), "plain", "1.0.1", {"tests/case.rs": b"//\n"})
            self.assertEqual(vendor_crates.development_files(other, policy), [])
            self.assertTrue((other / "tests/case.rs").is_file())

    def test_policy_cannot_select_production_sources_or_globs(self):
        with tempfile.TemporaryDirectory() as scratch:
            path = Path(scratch) / "policy.toml"
            for directories in (
                ["src"],
                ["../tests"],
                ["tests/**"],
                ["tests", "tests"],
                [],
            ):
                path.write_text(
                    '[development_directories]\n"plain-1.0.0" = '
                    + json.dumps(directories)
                )
                with (
                    self.subTest(directories=directories),
                    self.assertRaisesRegex(ValueError, "development directories"),
                ):
                    vendor_crates.load_development_policy(path)
            path.write_text('[development_directories]\n"plain-1.0.0" = ["tests"]')
            self.assertEqual(
                vendor_crates.load_development_policy(path), {"plain-1.0.0": ["tests"]}
            )


def files_of(root):
    """The files below a directory, with their bytes, by relative path."""
    return {
        path.relative_to(root).as_posix(): path.read_bytes()
        for path in root.rglob("*")
        if path.is_file()
    }


class StubTests(unittest.TestCase):
    """A crate that no supported target compiles keeps only its manifest."""

    def test_a_stub_keeps_its_manifest_and_its_checksum_entry(self):
        manifest = (
            b'[package]\nname = "wasm"\nversion = "1.0.0"\n\n'
            b'[lib]\npath = "src/lib.rs"\n'
        )
        files = {
            "Cargo.toml": manifest,
            "Cargo.toml.orig": b"[package]\n",
            "LICENSE-MIT": b"Copyright\n",
            "src/lib.rs": b"//\n",
            "src/rt/runtime.o": b"\x00asm\x01",
        }
        with tempfile.TemporaryDirectory() as scratch:
            root = crate(Path(scratch), "wasm", "1.0.0", files)
            left_out = vendor_crates.stub(root, root.name)
            self.assertEqual(
                left_out,
                ["Cargo.toml.orig", "LICENSE-MIT", "src/lib.rs", "src/rt/runtime.o"],
            )
            # The emptied directories go too, and Cargo finds the library
            # through the manifest's own path.
            names = sorted(path.name for path in root.iterdir())
            self.assertEqual(names, [".cargo-checksum.json", "Cargo.toml"])
            self.assertEqual((root / "Cargo.toml").read_bytes(), manifest)
            record = json.loads((root / ".cargo-checksum.json").read_text())
            # The package checksum, which Cargo compares with Cargo.lock, stays.
            self.assertEqual(
                record,
                {"$comment": "x", "files": {"Cargo.toml": "recorded"}, "package": "p"},
            )

    def test_a_manifest_without_a_library_path_gets_an_empty_library(self):
        manifest = b'[package]\nname = "keccak"\nversion = "2.0.2"\n'
        files = {"Cargo.toml": manifest, "src/lib.rs": b"//\n", "src/f.rs": b"//\n"}
        with tempfile.TemporaryDirectory() as scratch:
            root = crate(Path(scratch), "keccak", "2.0.2", files)
            self.assertEqual(
                vendor_crates.stub(root, root.name), ["src/f.rs", "src/lib.rs"]
            )
            # Cargo cannot read the manifest without src/lib.rs, which takes
            # the place of the crate's, empty and unrecorded.
            remaining = files_of(root)
            self.assertEqual(
                sorted(remaining),
                [".cargo-checksum.json", "Cargo.toml", vendor_crates.LIBRARY],
            )
            self.assertEqual(remaining["Cargo.toml"], manifest)
            self.assertEqual(remaining[vendor_crates.LIBRARY], b"")
            record = json.loads((root / ".cargo-checksum.json").read_text())
            self.assertEqual(record["files"], {"Cargo.toml": "recorded"})


def resolved(name, version, dependencies=(), macro=False):
    """A package and its resolve node, as `cargo metadata` gives them;
    `dependencies` are (package, kind) pairs."""
    identifier = f"{name} {version}"
    library = ["proc-macro"] if macro else ["lib"]
    package = {
        "id": identifier,
        "name": name,
        "version": version,
        "targets": [{"kind": library}],
    }
    node = {
        "id": identifier,
        "deps": [
            {"pkg": dependency, "dep_kinds": [{"kind": kind, "target": None}]}
            for dependency, kind in dependencies
        ],
    }
    return package, node


def metadata(*entries):
    """The `cargo metadata` output of resolved() packages."""
    return {
        "packages": [package for package, _ in entries],
        "resolve": {"nodes": [node for _, node in entries]},
    }


class CompiledTests(unittest.TestCase):
    def test_each_target_compiles_its_linked_and_build_time_packages(self):
        # `cargo tree` gives what each target links; `cargo metadata` resolves
        # every dependency for the target, the optional ones that no build
        # selects included.
        trees = {
            "first": ["app v0.1.0 (/checkout/app)", "linked v1.0.0"],
            "second": ["app v0.1.0 (/checkout/app)", "other v2.0.0 (*)"],
        }
        resolutions = {
            "first": metadata(
                resolved(
                    "app",
                    "0.1.0",
                    [
                        ("linked 1.0.0", None),
                        ("helper 1.0.0", "build"),
                        ("derive 1.0.0", None),
                        ("weak 1.0.0", None),
                    ],
                ),
                resolved("linked", "1.0.0"),
                resolved("helper", "1.0.0", [("host 1.0.0", None)]),
                resolved("host", "1.0.0"),
                resolved("derive", "1.0.0", [("syntax 1.0.0", None)], macro=True),
                resolved("syntax", "1.0.0"),
                resolved("weak", "1.0.0"),
            ),
            "second": metadata(
                resolved(
                    "app", "0.1.0", [("other 2.0.0", None), ("helper 1.0.0", "build")]
                ),
                resolved("other", "2.0.0"),
                resolved("helper", "1.0.0"),
            ),
        }
        calls = []

        def run(command, cwd, stdout, text, check):
            calls.append(command)
            if command[1] == "tree":
                output = "\n".join(trees[command[command.index("--target") + 1]])
            else:
                target = command[command.index("--filter-platform") + 1]
                output = json.dumps(resolutions[target])
            return subprocess.CompletedProcess(command, 0, stdout=output)

        with mock.patch.object(vendor_crates.subprocess, "run", run):
            found = vendor_crates.compiled(
                Path("workspace"), Path("crates"), ("first", "second")
            )
        self.assertEqual(
            found,
            {
                ("app", "0.1.0"),
                ("linked", "1.0.0"),
                ("other", "2.0.0"),
                # A build script's dependencies and a procedural macro, with
                # what they reach for the target.
                ("helper", "1.0.0"),
                ("host", "1.0.0"),
                ("derive", "1.0.0"),
                ("syntax", "1.0.0"),
            },
        )
        directory = json.dumps(str(Path("crates").absolute()), ensure_ascii=False)
        for command in calls:
            self.assertEqual(
                command[-6:],
                [
                    "--locked",
                    "--offline",
                    "--config",
                    'source.crates-io.replace-with="vendored-sources"',
                    "--config",
                    f"source.vendored-sources.directory={directory}",
                ],
            )
        self.assertEqual([command[1] for command in calls], ["tree", "metadata"] * 2)
        self.assertIn("normal,no-proc-macro", calls[0])


class ToolchainFileTests(unittest.TestCase):
    """No vendored crate keeps a file that makes rustup select a toolchain."""

    def test_toolchain_files_anywhere_leave_the_crate_and_its_record(self):
        files = {
            "src/lib.rs": b"//\n",
            "rust-toolchain.toml": b'[toolchain]\nchannel = "1.57.0"\n',
            "tests/fixture/rust-toolchain": b"nightly\n",
            "src/rust-toolchain.rs": b"//\n",
        }
        with tempfile.TemporaryDirectory() as scratch:
            root = crate(Path(scratch), "atoi", "2.0.0", files)
            found = vendor_crates.toolchain_files(root)
            self.assertEqual(
                found, ["rust-toolchain.toml", "tests/fixture/rust-toolchain"]
            )
            vendor_crates.leave_out(root, found, root.name)
            self.assertEqual(
                sorted(
                    path.relative_to(root).as_posix()
                    for path in root.rglob("*")
                    if path.is_file()
                ),
                [
                    ".cargo-checksum.json",
                    "Cargo.toml",
                    "src/lib.rs",
                    "src/rust-toolchain.rs",
                ],
            )
            text = (root / ".cargo-checksum.json").read_text()
            record = json.loads(text)
            # The package checksum, which Cargo compares with Cargo.lock, stays.
            self.assertEqual(
                record,
                {
                    "$comment": "x",
                    "files": {
                        "src/lib.rs": "recorded",
                        "src/rust-toolchain.rs": "recorded",
                    },
                    "package": "p",
                },
            )
            self.assertEqual(
                text, json.dumps(record, separators=(",", ":"), sort_keys=True)
            )

    def test_records_change_only_for_files_left_out(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = crate(Path(scratch), "plain", "1.0.0", {"src/lib.rs": b"//\n"})
            before = (root / ".cargo-checksum.json").read_bytes()
            self.assertEqual(vendor_crates.toolchain_files(root), [])
            vendor_crates.leave_out(root, [], root.name)
            self.assertEqual((root / ".cargo-checksum.json").read_bytes(), before)
            # An unrecorded file stops the removal before any file goes.
            (root / "rust-toolchain").write_text("stable\n")
            with self.assertRaisesRegex(ValueError, "does not list rust-toolchain"):
                vendor_crates.leave_out(
                    root, ["src/lib.rs", "rust-toolchain"], root.name
                )
            self.assertTrue((root / "rust-toolchain").is_file())
            self.assertTrue((root / "src/lib.rs").is_file())
            self.assertEqual((root / ".cargo-checksum.json").read_bytes(), before)


class VendorTests(unittest.TestCase):
    def vendor(self, populate, linked):
        """Run vendor() for one target with Cargo replaced: `cargo vendor` by
        populate(directory), and the target's resolution by one that links the
        (name, version) pairs `linked`. Returns the crates and the files of
        the vendor directory from which Cargo resolves the lock at the end."""
        remaining = {}

        def run(command, cwd, stdout, check, text=False):
            if command[1] == "vendor":
                self.assertEqual(
                    command[:4], ["cargo", "vendor", "--locked", "--versioned-dirs"]
                )
                populate(Path(command[-1]))
                return subprocess.CompletedProcess(command, 0)
            self.assertEqual(command[-5:-3], ["--offline", "--config"])
            if command[1] == "tree":
                output = "\n".join(f"{name} v{version}" for name, version in linked)
            elif "--filter-platform" in command:
                output = json.dumps(metadata())
            else:
                setting = command[-1].partition("=")[2]
                remaining.update(files_of(Path(json.loads(setting))))
                output = "{}"
            return subprocess.CompletedProcess(command, 0, stdout=output)

        with tempfile.TemporaryDirectory() as scratch:
            with mock.patch.object(vendor_crates.subprocess, "run", run):
                crates = vendor_crates.vendor(
                    Path(scratch), Path(scratch) / "vendor", ["target"]
                )
        return crates, remaining

    def test_development_and_toolchain_omissions_are_combined_without_duplicates(self):
        def populate(directory):
            crate(directory, "zstd-sys", "2.1.2", {"zstd/COPYING": b""})
            crate(
                directory,
                "plain",
                "1.0.0",
                {
                    "src/lib.rs": b"// production\n",
                    "tests/rust-toolchain": b"nightly\n",
                    "tests/case.rs": b"// test\n",
                },
            )

        with mock.patch.object(
            vendor_crates,
            "load_development_policy",
            return_value={"plain-1.0.0": ["tests"]},
        ):
            crates, remaining = self.vendor(
                populate, [("plain", "1.0.0"), ("zstd-sys", "2.1.2")]
            )
        self.assertIn(
            ("plain", "1.0.0", ["tests/case.rs", "tests/rust-toolchain"], True), crates
        )
        self.assertFalse(
            any(name.startswith("plain-1.0.0/tests/") for name in remaining)
        )
        self.assertEqual(remaining["plain-1.0.0/src/lib.rs"], b"// production\n")

    def test_crates_that_no_target_compiles_keep_only_their_manifests(self):
        wasm = b'[package]\nname = "wasm"\nversion = "0.1.0"\n'

        def populate(directory):
            crate(directory, "zeta", "1.0.0+extra", {"src/lib.rs": b"//\n"})
            crate(
                directory,
                "zstd-sys",
                "2.1.2",
                {"zstd/COPYING": b"text\n", "zstd/lib/zstd.h": b"/* BSD */\n"},
            )
            crate(
                directory,
                "wasm",
                "0.1.0",
                {
                    "Cargo.toml": wasm,
                    "src/rt/runtime.o": b"\x00asm\x01",
                    "rust-toolchain.toml": b"[toolchain]\n",
                },
            )
            crate(directory, "quote", "1.0.0", {"rust-toolchain.toml": b""})

        linked = [("zeta", "1.0.0+extra"), ("zstd-sys", "2.1.2"), ("quote", "1.0.0")]
        crates, remaining = self.vendor(populate, linked)
        self.assertEqual(
            crates,
            [
                ("quote", "1.0.0", ["rust-toolchain.toml"], True),
                ("wasm", "0.1.0", ["rust-toolchain.toml", "src/rt/runtime.o"], False),
                ("zeta", "1.0.0+extra", [], True),
                ("zstd-sys", "2.1.2", ["zstd/COPYING"], True),
            ],
        )
        # Cargo resolves the lock from the trimmed crates.
        self.assertEqual(
            sorted(name for name in remaining if name.startswith("wasm-0.1.0/")),
            [
                "wasm-0.1.0/.cargo-checksum.json",
                "wasm-0.1.0/Cargo.toml",
                "wasm-0.1.0/src/lib.rs",
            ],
        )
        self.assertEqual(remaining["wasm-0.1.0/src/lib.rs"], b"")
        self.assertNotIn("zstd-sys-2.1.2/zstd/COPYING", remaining)
        self.assertIn("zstd-sys-2.1.2/zstd/lib/zstd.h", remaining)

    def test_unexpected_vendor_output_is_refused(self):
        def misnamed(directory):
            root = crate(directory, "alpha", "1.0.0", {})
            root.rename(directory / "beta-1.0.0")

        def unrecorded(directory):
            crate(directory, "alpha", "1.0.0", {})
            (directory / "alpha-1.0.0/.cargo-checksum.json").unlink()

        def stale_table(directory):
            crate(directory, "alpha", "1.0.0", {})

        def binary(directory):
            stale_table(directory)
            crate(directory, "zstd-sys", "2.1.2", {"zstd/COPYING": b""})
            crate(directory, "tool", "1.0.0", {"bin/tool": b"\x7fELF\x02"})

        for populate, message in (
            (misnamed, "declares alpha 1.0.0"),
            (unrecorded, "has no .cargo-checksum.json"),
            (stale_table, "ALTERNATIVE_LICENSES lists zstd-sys"),
            (binary, "tool-1.0.0 ships the prebuilt binary bin/tool"),
        ):
            with self.subTest(case=populate.__name__):
                with self.assertRaisesRegex(ValueError, message):
                    self.vendor(populate, [("alpha", "1.0.0"), ("tool", "1.0.0")])
        # A binary in a crate that no target compiles leaves with the rest.
        crates, _ = self.vendor(binary, [("alpha", "1.0.0")])
        self.assertIn(("tool", "1.0.0", ["bin/tool"], False), crates)

    def test_the_script_runs_without_the_repository(self):
        # The R source package carries the script beside its policy, without
        # Python package dependencies or a checkout.
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
            (Path(scratch) / vendor_crates.POLICY.name).write_bytes(
                vendor_crates.POLICY.read_bytes()
            )
            result = subprocess.run(
                [sys.executable, "-I", str(copy), "--help"],
                cwd=scratch,
                capture_output=True,
                text=True,
                check=True,
            )
            subprocess.run(
                [
                    sys.executable,
                    "-I",
                    "-c",
                    "import runpy; assert runpy.run_path('vendor_crates.py')['load_development_policy']()",
                ],
                cwd=scratch,
                check=True,
            )
        self.assertIn("workspace", result.stdout)


class AuthorsTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        # zeta 1.9.0 offers a GNU license too, which yamaa does not rely on.
        self.licenses = {"zeta 1.9.0": "MIT OR Apache-2.0 OR LGPL-2.1-or-later"}

    def metadata(self):
        def package(name, version, source, **fields):
            directory = self.root / f"{name}-{version}"
            directory.mkdir(exist_ok=True)
            (directory / "LICENSE-MIT").write_text(
                f"Copyright {name}\n\nPermission is hereby granted, free of "
                "charge, to any person obtaining a copy\n"
            )
            return {
                "id": f"{name} {version}",
                "name": name,
                "version": version,
                "source": source,
                "authors": fields.get("authors", []),
                "license": self.licenses.get(f"{name} {version}", "MIT"),
                "repository": fields.get("repository"),
                "manifest_path": fields.get(
                    "manifest_path", str(directory / "Cargo.toml")
                ),
            }

        registry = staging.notices.CRATES_IO
        parser = staging.WORKSPACE / "vendor/saphyr-parser/Cargo.toml"
        return {
            "workspace_members": ["yamaa-r 0.5.0", "yamaa-python 0.5.0"],
            "packages": [
                package("yamaa-r", "0.5.0", None),
                package("yamaa-python", "0.5.0", None),
                package("zeta", "1.10.0", registry, authors=["Z <z@example.org>"]),
                package("zeta", "1.9.0", registry, repository="https://example.org/z"),
                package("alpha", "0.1.0", registry, authors=["A", "B"]),
                package("parser", "0.2.1", None, manifest_path=str(parser)),
                # The canonical graph's Python binding crates, outside the R lock.
                package("pyo3", "0.27.2", registry),
            ],
        }

    # The (name, version, source) packages of the R bundle's trimmed lock.
    LOCKED = {
        ("yamaa-r", "0.5.0", None),
        ("zeta", "1.10.0", staging.notices.CRATES_IO),
        ("zeta", "1.9.0", staging.notices.CRATES_IO),
        ("alpha", "0.1.0", staging.notices.CRATES_IO),
        ("parser", "0.2.1", None),
    }

    def test_every_bundled_crate_is_named_with_its_declared_facts(self):
        crates = [
            ("alpha", "0.1.0", ["zstd/COPYING"], True),
            ("zeta", "1.9.0", ["src/lib.rs"], False),
            ("zeta", "1.10.0", [], True),
        ]
        # The notices list only the crates that the R binding links.
        listed = {("alpha", "0.1.0"), ("zeta", "1.10.0"), ("parser", "0.2.1")}
        text = staging.authors_text(self.metadata(), crates, listed, self.LOCKED)
        entries = text.split("\nCrate: ")[1:]
        self.assertEqual(
            [entry.splitlines()[0] for entry in entries],
            ["alpha 0.1.0", "parser 0.2.1", "zeta 1.9.0", "zeta 1.10.0"],
        )
        self.assertIn("    Authors: A, B\n", entries[0])
        self.assertIn("    Left out: zstd/COPYING\n", entries[0])
        self.assertIn("    Directory: src/rust/vendor/saphyr-parser\n", entries[1])
        self.assertIn("    Authors: none declared\n", entries[2])
        self.assertIn("    Repository: https://example.org/z\n", entries[2])
        # A crate that no supported target compiles keeps its manifest alone.
        self.assertIn("    Left out: every file but its manifest\n", entries[2])
        self.assertNotIn("Left out", entries[3])
        self.assertIn("    Repository: none declared\n", entries[3])
        # Each entry names the one license that yamaa relies on.
        self.assertIn("    License: MIT\n", entries[2])
        self.assertNotIn(" OR ", text)
        self.assertNotIn("yamaa-r", text)
        # A canonical package outside the bundle's lock is not bundled.
        self.assertNotIn("pyo3", text)
        self.assertTrue(text.endswith("\n"))

    def test_unvendored_or_unlicensed_crates_stop_staging(self):
        crates = [
            ("alpha", "0.1.0", [], True),
            ("zeta", "1.9.0", [], True),
            ("zeta", "1.10.0", [], True),
        ]
        listed = {
            ("alpha", "0.1.0"),
            ("zeta", "1.9.0"),
            ("zeta", "1.10.0"),
            ("parser", "0.2.1"),
        }
        with self.assertRaisesRegex(
            ValueError, "zeta 1.10.0: the vendored crates differ"
        ):
            staging.authors_text(self.metadata(), crates[:2], listed, self.LOCKED)
        # A bundled crate that offers only a license that yamaa does not
        # accept stops it.
        self.licenses["alpha 0.1.0"] = "LGPL-2.1-or-later"
        with self.assertRaisesRegex(ValueError, "alpha 0.1.0: review the license"):
            staging.authors_text(self.metadata(), crates, listed, self.LOCKED)
        del self.licenses["alpha 0.1.0"]
        # Notices that name a crate the package does not bundle, such as the
        # canonical graph's Python binding crates, stop it too.
        with self.assertRaisesRegex(
            ValueError,
            "pyo3 0.27.2: LICENSE-THIRD-PARTY.txt lists it, but the package does "
            "not bundle it",
        ):
            staging.authors_text(
                self.metadata(), crates, listed | {("pyo3", "0.27.2")}, self.LOCKED
            )
        # A vendored crate that the trimmed lock does not select stops it too.
        with self.assertRaisesRegex(ValueError, "pyo3 0.27.2: the vendored"):
            staging.authors_text(
                self.metadata(),
                [*crates, ("pyo3", "0.27.2", [], True)],
                listed,
                self.LOCKED,
            )


if __name__ == "__main__":
    unittest.main()
