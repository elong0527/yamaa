"""The conformance archive holds the closed grammars, the expression language's
evaluation set and the project license, as committed."""

import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock

from tools.packaging import cli_archive
from tools.packaging import conformance_archive as archives
from tools.paths import ROOT
from tools.tests import support
from tools.validation import check_expression_conformance


class Files(unittest.TestCase):
    def test_the_files_are_the_grammars_the_evaluation_set_and_the_license(self):
        # yaml/grammar/README.md: each closed grammar of the language is
        # defined once, there. The evaluation set is the document that
        # `make check-repository` runs through the engine.
        grammars = sorted(
            path.relative_to(ROOT).as_posix()
            for path in (ROOT / "yaml/grammar").glob("*.yaml")
        )
        self.assertEqual(
            archives.FILES,
            (
                *grammars,
                check_expression_conformance.DOCUMENT.as_posix(),
                "LICENSE",
            ),
        )


class Archives(unittest.TestCase):
    """A checkout's conformance files, archived and checked."""

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        (self.root / "tools/packaging").mkdir(parents=True)
        (self.root / "tools/packaging/release.toml").write_text(
            'version = "1.0.0"\ncandidate = 1\n'
        )
        self.files = {file: f"# {file}\n".encode() for file in archives.FILES}
        for file, payload in self.files.items():
            (self.root / file).parent.mkdir(parents=True, exist_ok=True)
            (self.root / file).write_bytes(payload)
        self.directory = "yamaa-1.0.0-rc.1-conformance"

    def write(self):
        return archives.write(self.root / "out", self.root)

    def rewrite(self, path, change):
        """Rewrite an archive's entries through `change`, which takes and returns
        a list of (ZipInfo, payload)."""
        with zipfile.ZipFile(path) as source:
            entries = [(entry, source.read(entry)) for entry in source.infolist()]
        with zipfile.ZipFile(path, "w") as target:
            for entry, payload in change(entries):
                target.writestr(entry, payload)

    def test_the_archive_holds_each_file_at_its_path_in_one_directory(self):
        path = self.write()
        self.assertEqual(path, self.root / "out" / f"{self.directory}.zip")
        with zipfile.ZipFile(path) as archive:
            self.assertEqual(
                {
                    entry.filename: (
                        archive.read(entry),
                        entry.create_system,
                        entry.external_attr >> 16,
                    )
                    for entry in archive.infolist()
                },
                {
                    f"{self.directory}/{file}": (payload, cli_archive.UNIX, 0o100644)
                    for file, payload in self.files.items()
                },
            )
        archives.check(path, self.root)
        # An archive is never overwritten.
        with self.assertRaises(FileExistsError):
            self.write()

    def test_an_archive_holds_exactly_the_committed_files_with_their_mode(self):
        path = self.write()
        original = path.read_bytes()
        file = "yaml/conformance/expressions.yaml"
        member = f"{self.directory}/{file}"
        notice = f"{self.directory}/LICENSE"

        def edited(entries):
            for entry, payload in entries:
                yield (
                    entry,
                    payload + b"# edited\n" if entry.filename == member else payload,
                )

        def mode(value):
            def change(entries):
                for entry, payload in entries:
                    if entry.filename == member:
                        entry.external_attr = value << 16
                    yield entry, payload

            return change

        def system(entries):
            for entry, payload in entries:
                entry.create_system = 0
                yield entry, payload

        def extra(entries):
            yield from entries
            info = zipfile.ZipInfo(f"{self.directory}/yaml/grammar/README.md")
            info.create_system = cli_archive.UNIX
            info.external_attr = archives.MODE << 16
            yield info, b"read me\n"

        def missing(entries):
            for entry, payload in entries:
                if entry.filename != notice:
                    yield entry, payload

        def outside(entries):
            for entry, payload in entries:
                if entry.filename == notice:
                    entry.filename = "LICENSE"
                yield entry, payload

        for case, change, error in (
            ("an edited file", edited, f"{member} differs from {file} in the checkout"),
            (
                "an executable file",
                mode(0o100755),
                f"{member} must be a regular file with Unix mode 644",
            ),
            (
                "a symbolic link",
                mode(0o120777),
                f"{member} must be a regular file with Unix mode 644",
            ),
            ("no Unix permissions", system, "must be a regular file"),
            ("another file", extra, "must hold exactly"),
            ("no license", missing, "must hold exactly"),
            ("a file outside the directory", outside, "must hold exactly"),
        ):
            self.rewrite(path, change)
            try:
                with self.subTest(case=case), self.assertRaisesRegex(ValueError, error):
                    archives.check(path, self.root)
            finally:
                path.write_bytes(original)
        archives.check(path, self.root)

    def test_only_the_release_names_its_archive(self):
        path = self.write()
        for name in (
            "yamaa-1.0.0rc1-conformance.zip",
            "yamaa-1.0.0-conformance.zip",
            "yamaa-1.0.0-rc.2-conformance.zip",
            "yamaa-1.0.0-rc.1-conformance.tar.gz",
        ):
            renamed = path.with_name(name)
            path.rename(renamed)
            try:
                with (
                    self.subTest(name=name),
                    self.assertRaisesRegex(
                        ValueError,
                        "not the yamaa 1.0.0 release candidate 1 conformance archive",
                    ),
                ):
                    archives.check(renamed, self.root)
            finally:
                renamed.rename(path)

    def test_the_command_writes_and_checks_an_archive(self):
        def run(*arguments):
            with mock.patch.object(archives, "ROOT", self.root):
                return support.run_main(archives.main, *arguments)

        code, output, _ = run("write", str(self.root / "out"))
        path = self.root / "out" / f"{self.directory}.zip"
        self.assertEqual((code, output), (0, f"{path}\n"))
        code, output, _ = run("check", str(path))
        self.assertEqual(
            (code, output),
            (
                0,
                f"{path.name}: yaml/grammar/expression.yaml, "
                "yaml/grammar/string-template.yaml, "
                "yaml/conformance/expressions.yaml, LICENSE, as committed\n",
            ),
        )
        other = self.root / f"{self.directory}.zip"
        other.write_bytes(b"not a ZIP archive")
        code, _, errors = run("check", str(other))
        self.assertEqual(code, 1)
        self.assertIn("not a zip file", errors.lower())


if __name__ == "__main__":
    unittest.main()
