"""The facade's selftest table and the case files that every bundle stages."""

import re
import tempfile
import unittest
from pathlib import Path

import yaml
from tools.packaging import generate_selftest_cases as generator
from tools.paths import ROOT

METADATA = {"lifecycle": "draft", "standard": "TEST", "domain": "TEST"}
NAMED = re.compile(r'case_file!\("([^"]+)"\)')


class CommittedTable(unittest.TestCase):
    def test_the_committed_table_matches_the_list_and_its_cases(self):
        self.assertEqual(
            generator.TARGET.read_text(encoding="ascii"), generator.render()
        )

    def test_the_table_names_each_staged_file_once_and_holds_no_bytes(self):
        text = generator.TARGET.read_text(encoding="ascii")
        staged = [relative for relative, _ in generator.case_files()]
        self.assertEqual(sorted(NAMED.findall(text)), sorted(staged))
        self.assertEqual(len(staged), len(set(staged)))
        # The expected bytes keep one source: the table embeds them from
        # benchmarks/ when the crate builds, and writes none itself.
        self.assertNotIn('b"', text)
        for relative, path in generator.case_files():
            self.assertEqual(path, ROOT / "benchmarks" / relative)
            self.assertNotIn(path.name, generator.LEFT_OUT)
            self.assertTrue(path.is_file(), relative)

    def test_each_case_carries_its_features_entries_and_submission(self):
        listed = yaml.safe_load(
            (ROOT / "benchmarks/selftest.yaml").read_text(encoding="utf-8")
        )
        cases = generator.cases()
        self.assertEqual([case["name"] for case in cases], listed["cases"])
        for case in cases:
            with self.subTest(case=case["name"]):
                self.assertIn("yaml", case["features"])
                self.assertEqual(
                    set(case["features"]) - {"yaml"},
                    set(case["adapters"]) & set(generator.FEATURES),
                )
                self.assertTrue(case["entries"])
                self.assertTrue(case["expected"])
        negative = [case for case in cases if "expected/error.yaml" in case["expected"]]
        self.assertTrue(negative)


class FakeCheckout(unittest.TestCase):
    """A list of three cases that cover every adapter but `odm` and
    `readstat`, one of them negative."""

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.case(
            "adam-json", "input/raw.csv", "expected/adae.json", "expected/define.xml"
        )
        self.case("adam-xpt", "input/raw.csv", "expected/adsl.csv", "expected/adsl.xpt")
        self.case("negative-parquet", "input/raw.parquet", "expected/error.yaml")
        self.write(
            {
                "cases": ["adam-json", "adam-xpt", "negative-parquet"],
                "uncovered": {
                    "odm": "no case reads ODM XML",
                    "readstat": "no case reads SAS files",
                },
            }
        )

    def case(self, name, *files, metadata=METADATA):
        directory = self.root / "benchmarks" / name
        for path in ("spec.yaml", "README.md", *files):
            (directory / path).parent.mkdir(parents=True, exist_ok=True)
            (directory / path).write_text("domain: TEST\n")
        (directory / "benchmark.yaml").write_text(yaml.safe_dump(metadata))
        return directory

    def write(self, document):
        (self.root / "benchmarks/selftest.yaml").write_text(yaml.safe_dump(document))

    def test_the_table_names_every_file_but_the_prose_and_the_metadata(self):
        text = generator.render(self.root)
        self.assertEqual(
            sorted(NAMED.findall(text)),
            sorted(relative for relative, _ in generator.case_files(self.root)),
        )
        self.assertEqual(
            [relative for relative, _ in generator.case_files(self.root)][:4],
            [
                "adam-json/expected/adae.json",
                "adam-json/expected/define.xml",
                "adam-json/input/raw.csv",
                "adam-json/spec.yaml",
            ],
        )
        self.assertNotIn("README.md", text)
        self.assertNotIn("benchmark.yaml", text)
        self.assertIn('Entry { specification: "spec.yaml", project: None }', text)
        self.assertIn('features: &["parquet", "yaml"]', text)
        self.assertIn(
            'pub(super) const UNCOVERED: &[&str] = &["odm", "readstat"];', text
        )

    def test_a_declared_submission_is_written_with_its_creation_time(self):
        case = self.root / "benchmarks/adam-xpt"
        (case / "project.yaml").write_text('yamaa: "1.0"\ntype: project\n')
        define = {
            "environment": "project.yaml",
            "standard": "adam",
            "created": "2026-09-11T00:00:00",
        }
        (case / "benchmark.yaml").write_text(
            yaml.safe_dump({**METADATA, "define": define})
        )
        text = generator.render(self.root)
        self.assertIn(
            'define: Some(Define { project: "project.yaml", section: "adam", '
            'created: "2026-09-11T00:00:00" }),',
            text,
        )
        # A root project is every entry's project, as the runners pass it.
        self.assertIn(
            'Entry { specification: "spec.yaml", project: Some("project.yaml") }',
            text,
        )

    def test_entries_that_differ_by_host_are_refused(self):
        case = self.root / "benchmarks/adam-json"
        (case / "other.yaml").write_text("domain: TEST\n")
        entries = {
            "python": [{"specification": "spec.yaml"}],
            "r": [{"specification": "other.yaml"}],
        }
        (case / "benchmark.yaml").write_text(
            yaml.safe_dump({**METADATA, "entries": entries})
        )
        with self.assertRaisesRegex(ValueError, "the same entries in every host"):
            generator.render(self.root)

    def test_a_list_that_its_check_refuses_is_not_written(self):
        self.write({"cases": ["adam-json", "adam-xpt"]})
        with self.assertRaisesRegex(ValueError, "no negative case"):
            generator.render(self.root)

    def test_prose_at_any_depth_is_left_out_and_expected_files_sit_flat(self):
        case = self.root / "benchmarks/adam-json"
        (case / "input/README.md").write_text("prose\n")
        staged = [relative for relative, _ in generator.case_files(self.root)]
        self.assertIn("adam-json/input/raw.csv", staged)
        self.assertNotIn("adam-json/input/README.md", staged)
        # The runners compare an expected file by its name alone.
        (case / "expected/more").mkdir()
        (case / "expected/more/adae.csv").write_text("A\n")
        with self.assertRaisesRegex(ValueError, "directly in expected/"):
            list(generator.case_files(self.root))

    def test_staging_refuses_a_link_an_odd_name_or_a_bad_list(self):
        case = self.root / "benchmarks/adam-json"
        quoted = case / 'input/"raw".csv'
        quoted.write_text("x\n")
        with self.assertRaisesRegex(ValueError, "unsupported file name"):
            list(generator.case_files(self.root))
        quoted.unlink()
        try:
            (case / "input/link.csv").symlink_to(case / "input/raw.csv")
        except OSError as error:
            self.skipTest(f"symlinks are unavailable: {error}")
        with self.assertRaisesRegex(ValueError, "expected a regular file"):
            list(generator.case_files(self.root))
        (case / "input/link.csv").unlink()
        for document in ({"uncovered": {}}, {"cases": ["../adam-json"]}):
            with self.subTest(document=document):
                self.write(document)
                with self.assertRaisesRegex(ValueError, "selftest.yaml"):
                    list(generator.case_files(self.root))


if __name__ == "__main__":
    unittest.main()
