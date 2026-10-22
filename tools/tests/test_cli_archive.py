"""Each command-line archive holds its target's executable and the notices."""

import struct
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock

from tools.ci import native
from tools.packaging import check_package_notices as notices
from tools.packaging import cli_archive as archives
from tools.packaging.release_identity import Identity
from tools.tests import support


def executable(target, marker=b"fixture"):
    """Bytes whose header names the format and architecture of the target's
    executable."""
    kind, architecture = archives.expected_format(target)
    machine = {name: code for code, name in archives.MACHINES[kind].items()}[
        architecture
    ]
    if kind == "ELF":
        header = b"\x7fELF\x02\x01" + bytes(12) + struct.pack("<H", machine)
    elif kind == "Mach-O":
        header = b"\xcf\xfa\xed\xfe" + struct.pack("<I", machine)
    else:
        header = b"MZ" + bytes(58) + struct.pack("<I", 64) + b"PE\0\0"
        header += struct.pack("<H", machine)
    return header + bytes(8) + marker


class Formats(unittest.TestCase):
    def test_each_target_names_its_executable_and_format(self):
        self.assertEqual(
            {
                target: (archives.executable(target), archives.expected_format(target))
                for target in native.CLI_TARGETS.values()
            },
            {
                "x86_64-unknown-linux-musl": ("yamaa", ("ELF", "x86_64")),
                "aarch64-unknown-linux-musl": ("yamaa", ("ELF", "aarch64")),
                "x86_64-pc-windows-msvc": ("yamaa.exe", ("PE", "x86_64")),
                "aarch64-apple-darwin": ("yamaa", ("Mach-O", "aarch64")),
            },
        )
        with self.assertRaisesRegex(ValueError, "not a command-line target"):
            archives.expected_format("wasm32-wasip2")

    def test_the_header_names_the_format_and_architecture(self):
        for target in native.CLI_TARGETS.values():
            with self.subTest(target=target):
                self.assertEqual(
                    archives.executable_format(executable(target)),
                    archives.expected_format(target),
                )
        elf = executable("x86_64-unknown-linux-musl")
        pe = executable("x86_64-pc-windows-msvc")
        for case, payload in (
            ("32-bit ELF", elf[:4] + b"\x01" + elf[5:]),
            ("big-endian ELF", elf[:5] + b"\x02" + elf[6:]),
            ("another machine", elf[:18] + struct.pack("<H", 3) + elf[20:]),
            ("a 32-bit Mach-O file", b"\xce\xfa\xed\xfe" + bytes(16)),
            ("a DOS stub without a PE header", pe[:64] + b"NE\0\0" + pe[68:]),
            ("a PE header past the end", pe[:60] + struct.pack("<I", 4096)),
            ("a truncated ELF header", elf[:12]),
            ("a script", b"#!/bin/sh\nexit 0\n"),
            ("nothing", b""),
        ):
            with self.subTest(case=case):
                self.assertIsNone(archives.executable_format(payload))


class Archives(unittest.TestCase):
    """A checkout's command-line notices, archived and checked."""

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        (self.root / "tools/packaging").mkdir(parents=True)
        (self.root / "tools/packaging/release.toml").write_text(
            'version = "1.0.0"\ncandidate = 1\n'
        )
        (self.root / "rust").mkdir()
        (self.root / "rust/rust-toolchain.toml").write_text(
            '[toolchain]\nchannel = "1.99.1"\n'
        )
        (self.root / "python").mkdir()
        (self.root / "python/pyproject.toml").write_text(
            '[project]\nversion = "1.0.0rc1"\n'
            'license-files = ["LICENSE.txt", "LICENSE-THIRD-PARTY.txt", '
            '"LICENSE-YAML.txt", "COMPONENTS.tsv"]\n'
        )
        shared = {"LICENSE.txt": b"project\n", "LICENSE-YAML.txt": b"yaml\n"}
        own = {
            "LICENSE-THIRD-PARTY.txt": b"Crate: arrow 55.2.0\n",
            "COMPONENTS.tsv": b"name\narrow\t55.2.0\nrust-std\t1.99.1\n",
        }
        # musl's notice, which only the archives for the musl targets carry.
        self.musl = {"LICENSE-MUSL.txt": b"musl 1.2.5\nMIT\n"}
        # The Python packages' own notices, which the command line does not
        # carry.
        python = {
            "LICENSE-THIRD-PARTY.txt": b"Crate: pyo3 0.27.2\n",
            "COMPONENTS.tsv": b"name\npyo3\t0.27.2\nrust-std\t1.99.1\n",
        }
        for directory, files in (
            ("python", {**shared, **python}),
            ("rust/crates/yamaa-cli", {**own, **self.musl}),
        ):
            (self.root / directory).mkdir(parents=True, exist_ok=True)
            for name, payload in files.items():
                (self.root / directory / name).write_bytes(payload)
        self.expected = {**shared, **own}
        self.identity = Identity("1.0.0", 1)

    def notices(self, target):
        """The notices of the target's archive, by name."""
        if target.endswith("-linux-musl"):
            return {**self.expected, **self.musl}
        return self.expected

    def write(self, target, payload=None):
        binary = self.root / "build" / archives.executable(target)
        binary.parent.mkdir(exist_ok=True)
        binary.write_bytes(executable(target) if payload is None else payload)
        return archives.write(target, binary, self.root / "out", self.root)

    def rewrite(self, path, change):
        """Rewrite an archive's entries through `change`, which takes and returns
        a list of (ZipInfo, payload)."""
        with zipfile.ZipFile(path) as source:
            entries = [(entry, source.read(entry)) for entry in source.infolist()]
        with zipfile.ZipFile(path, "w") as target:
            for entry, payload in change(entries):
                target.writestr(entry, payload)

    def test_each_target_archives_its_executable_beside_the_notices(self):
        for target in native.CLI_TARGETS.values():
            with self.subTest(target=target):
                path = self.write(target)
                directory = f"yamaa-1.0.0-rc.1-{target}"
                self.assertEqual(path.name, f"{directory}.zip")
                expected = self.notices(target)
                with zipfile.ZipFile(path) as archive:
                    entries = {entry.filename: entry for entry in archive.infolist()}
                    program = f"{directory}/{archives.executable(target)}"
                    self.assertEqual(
                        sorted(entries),
                        sorted(
                            [program, *(f"{directory}/{name}" for name in expected)]
                        ),
                    )
                    self.assertEqual(archive.read(program), executable(target))
                    for name, payload in expected.items():
                        self.assertEqual(archive.read(f"{directory}/{name}"), payload)
                # unzip restores these Unix modes on Linux and macOS.
                for name, entry in entries.items():
                    self.assertEqual(entry.create_system, archives.UNIX)
                    self.assertEqual(
                        entry.external_attr >> 16,
                        archives.EXECUTABLE_MODE
                        if name == program
                        else archives.NOTICE_MODE,
                    )
                self.assertEqual(archives.check(path, self.root), target)
                self.assertEqual(
                    len(notices.check_archive(path, self.root)), len(expected)
                )
        # Only the archives for the musl targets carry musl's notice.
        self.assertEqual(
            sorted(
                target
                for target in native.CLI_TARGETS.values()
                if "LICENSE-MUSL.txt"
                in archives.layout(self.identity, target, self.root)[1]
            ),
            ["aarch64-unknown-linux-musl", "x86_64-unknown-linux-musl"],
        )

    def test_only_an_executable_for_the_named_target_is_archived(self):
        for target, payload, error in (
            (
                "x86_64-unknown-linux-musl",
                executable("aarch64-unknown-linux-musl"),
                "not the yamaa executable for x86_64-unknown-linux-musl",
            ),
            (
                "aarch64-apple-darwin",
                b"#!/bin/sh\n",
                "not the yamaa executable for aarch64-apple-darwin",
            ),
            (
                "x86_64-unknown-linux-gnu",
                executable("x86_64-unknown-linux-musl"),
                "x86_64-unknown-linux-gnu: not a command-line target",
            ),
        ):
            with self.subTest(target=target), self.assertRaisesRegex(ValueError, error):
                self.write(target, payload)
        self.assertFalse((self.root / "out").exists())
        # An archive is never overwritten.
        self.write("aarch64-apple-darwin")
        with self.assertRaises(FileExistsError):
            self.write("aarch64-apple-darwin")

    def test_an_archive_holds_exactly_its_members_with_their_modes(self):
        target = "x86_64-unknown-linux-musl"
        program = f"yamaa-1.0.0-rc.1-{target}/yamaa"
        path = self.write(target)
        original = path.read_bytes()

        def mode(name, value):
            def change(entries):
                for entry, payload in entries:
                    if entry.filename == name:
                        entry.external_attr = value << 16
                    yield entry, payload

            return change

        def system(entries):
            for entry, payload in entries:
                entry.create_system = 0
                yield entry, payload

        def extra(entries):
            yield from entries
            info = zipfile.ZipInfo(f"yamaa-1.0.0-rc.1-{target}/README")
            info.create_system = archives.UNIX
            info.external_attr = archives.NOTICE_MODE << 16
            yield info, b"read me\n"

        def missing(entries, name="LICENSE-YAML.txt"):
            for entry, payload in entries:
                if not entry.filename.endswith(f"/{name}"):
                    yield entry, payload

        def missing_musl(entries):
            return missing(entries, "LICENSE-MUSL.txt")

        def foreign(entries):
            for entry, payload in entries:
                if entry.filename == program:
                    payload = executable("x86_64-pc-windows-msvc")
                yield entry, payload

        notice = f"yamaa-1.0.0-rc.1-{target}/LICENSE.txt"
        for case, change, error in (
            (
                "a notice that is executable",
                mode(notice, archives.EXECUTABLE_MODE),
                "LICENSE.txt must be a regular file with Unix mode 644",
            ),
            (
                "an executable without its mode",
                mode(program, archives.NOTICE_MODE),
                "yamaa must be a regular file with Unix mode 755",
            ),
            (
                "a symbolic link",
                mode(program, 0o120777),
                "yamaa must be a regular file with Unix mode 755",
            ),
            ("no Unix permissions", system, "must be a regular file"),
            ("another file", extra, "must hold exactly"),
            ("a missing notice", missing, "must hold exactly"),
            ("no notice of musl", missing_musl, "must hold exactly"),
            (
                "another target's executable",
                foreign,
                f"yamaa is not the yamaa executable for {target}",
            ),
        ):
            self.rewrite(path, change)
            try:
                with (
                    self.subTest(case=case),
                    self.assertRaisesRegex(ValueError, error),
                ):
                    archives.check(path, self.root)
            finally:
                path.write_bytes(original)
        self.assertEqual(archives.check(path, self.root), target)

    def test_an_archive_for_another_target_holds_no_notice_of_musl(self):
        # The macOS executable links no musl, so its archive carries no notice
        # of it.
        target = "aarch64-apple-darwin"
        path = self.write(target)

        def musl(entries):
            yield from entries
            info = zipfile.ZipInfo(f"yamaa-1.0.0-rc.1-{target}/LICENSE-MUSL.txt")
            info.create_system = archives.UNIX
            info.external_attr = archives.NOTICE_MODE << 16
            yield info, self.musl["LICENSE-MUSL.txt"]

        self.rewrite(path, musl)
        with self.assertRaisesRegex(ValueError, "must hold exactly"):
            archives.check(path, self.root)

    def test_only_the_release_and_a_declared_target_name_an_archive(self):
        path = self.write("aarch64-apple-darwin")
        for name in (
            "yamaa-1.0.0rc1-aarch64-apple-darwin.zip",
            "yamaa-1.0.0-aarch64-apple-darwin.zip",
            "yamaa-1.0.0-rc.1-x86_64-apple-darwin.zip",
            "yamaa-1.0.0-rc.1-aarch64-apple-darwin.tar.gz",
        ):
            renamed = path.with_name(name)
            path.rename(renamed)
            try:
                with (
                    self.subTest(name=name),
                    self.assertRaisesRegex(
                        ValueError,
                        "not a yamaa 1.0.0 release candidate 1 command-line archive",
                    ),
                ):
                    archives.check(renamed, self.root)
            finally:
                renamed.rename(path)

    def test_the_command_writes_and_checks_archives(self):
        binary = self.root / "yamaa.exe"
        binary.write_bytes(executable("x86_64-pc-windows-msvc"))

        def run(*arguments):
            with mock.patch.object(archives, "ROOT", self.root):
                return support.run_main(archives.main, *arguments)

        code, output, _ = run(
            "write", "x86_64-pc-windows-msvc", str(binary), str(self.root / "out")
        )
        path = self.root / "out/yamaa-1.0.0-rc.1-x86_64-pc-windows-msvc.zip"
        self.assertEqual((code, output), (0, f"{path}\n"))
        code, output, _ = run("check", str(path))
        self.assertEqual(
            (code, output),
            (0, f"{path.name}: the yamaa executable for x86_64-pc-windows-msvc\n"),
        )
        other = self.root / "yamaa-1.0.0-rc.1-x86_64-pc-windows-msvc.zip"
        other.write_bytes(b"not a ZIP archive")
        code, _, errors = run("check", str(other))
        self.assertEqual(code, 1)
        self.assertIn("not a zip file", errors.lower())


if __name__ == "__main__":
    unittest.main()
