"""The shipped schema: the copies of the schema files that the engine embeds and
the table that names them, kept to yaml/ without content hashes."""

import re
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.packaging import generate_shipped_schema as generator
from tools.paths import ROOT
from tools.tests import support

# One line of the table: an embedded file's name and the copy it reads.
ENTRY = re.compile(r'    \("([^"]+)", include_str!\("\.\./\.\./schema/([^"]+)"\)\),')


class ShippedSchemaTests(unittest.TestCase):
    def test_the_table_and_the_copies_hold_the_bytes_of_yaml(self):
        # The committed state, as `make check-repository` compares it.
        files = generator.expected()
        for path, payload in files.items():
            with self.subTest(path=path.relative_to(ROOT).as_posix()):
                self.assertEqual(path.read_bytes(), payload)
        self.assertEqual(generator.strays(files), [])

    def test_each_table_entry_reads_the_copy_of_its_own_file(self):
        # Every schema module by its own name, and each file type's JSON
        # Schema by the type that `schema(file_type)` names, one line each,
        # read from schema/, beside the table's src/shipped_schema/.
        modules = sorted(path.name for path in (ROOT / "yaml").glob("schema_*.yaml"))
        types = ("spec", "project", "codelists", "function")
        lines = [
            line for line in generator.render().splitlines() if "include_str!" in line
        ]
        self.assertEqual(
            [
                match.groups() if (match := ENTRY.fullmatch(line)) else line
                for line in lines
            ],
            [(name, name) for name in modules]
            + [(name, f"json/{name}.schema.json") for name in types],
        )

    def test_a_changed_or_stray_copy_is_refused_until_write_restores_the_copies(
        self,
    ):
        # The check and `--write` themselves, on a scratch crate that copies
        # the repository's yaml/.
        with tempfile.TemporaryDirectory() as directory:
            crate = Path(directory) / "yamaa-adapters"
            copies = crate / "schema"
            with mock.patch.multiple(
                generator, COPIES=copies, TARGET=crate / "src/shipped_schema/data.rs"
            ):
                self.assertEqual(support.run_main(generator.main, "--write")[0], 0)
                self.assertEqual(support.run_main(generator.main)[0], 0)
                changed = copies / "schema_spec.yaml"
                changed.write_bytes(changed.read_bytes() + b"# edited\n")
                stray = support.write(copies, "schema_retired.yaml", 'version: "1.0"\n')
                self.assertEqual(
                    support.run_main(generator.main),
                    (
                        1,
                        f"ERROR: regenerate {changed}\n"
                        f"ERROR: {stray} copies no schema file\n"
                        "Run: python -m tools.packaging.generate_shipped_schema "
                        "--write\n",
                        "",
                    ),
                )
                self.assertEqual(support.run_main(generator.main, "--write")[0], 0)
                self.assertFalse(stray.exists())
                self.assertEqual(
                    changed.read_bytes(), (ROOT / "yaml/schema_spec.yaml").read_bytes()
                )
                self.assertEqual(support.run_main(generator.main)[0], 0)

    def test_a_linked_copy_is_refused_until_write_replaces_it_with_the_file(self):
        # The check and `--write` on a link, which a byte comparison would read
        # through: `--write` removes it rather than write through it.
        with tempfile.TemporaryDirectory() as directory:
            crate = Path(directory) / "yamaa-adapters"
            copies = crate / "schema"
            source = ROOT / "yaml/schema_spec.yaml"
            with mock.patch.multiple(
                generator, COPIES=copies, TARGET=crate / "src/shipped_schema/data.rs"
            ):
                self.assertEqual(support.run_main(generator.main, "--write")[0], 0)
                linked = copies / "schema_spec.yaml"
                linked.unlink()
                try:
                    linked.symlink_to(source)
                except OSError as error:
                    self.skipTest(f"symlinks are unavailable: {error}")
                self.assertEqual(
                    support.run_main(generator.main),
                    (
                        1,
                        f"ERROR: {linked} is a link, not a copy\n"
                        "Run: python -m tools.packaging.generate_shipped_schema "
                        "--write\n",
                        "",
                    ),
                )
                self.assertEqual(support.run_main(generator.main, "--write")[0], 0)
                self.assertEqual(
                    (linked.is_symlink(), linked.read_bytes()),
                    (False, source.read_bytes()),
                )
                self.assertEqual(support.run_main(generator.main)[0], 0)


if __name__ == "__main__":
    unittest.main()
