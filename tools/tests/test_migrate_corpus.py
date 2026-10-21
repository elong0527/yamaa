"""The converter over the whole benchmark corpus: properties that hold for any corpus.

Every written file parses and keeps its comments, the follow-up list names what
the conversion leaves to people, and a second run changes nothing. The counts
the converter produced against the RFC's measurement of main 1758ad145 were
the acceptance check of #3165 and are recorded on its pull request (#3186);
they are a snapshot of a moving corpus, so this suite does not pin them.
"""

import tempfile
import unittest
from collections import Counter
from pathlib import Path

import yaml
from tools.migrate import convert, write
from tools.migrate.text import comment_texts
from tools.paths import ROOT

BENCHMARKS = ROOT / "benchmarks"


def zero_five_corpus():
    """True while every benchmark specification still declares schema_version."""
    for path in BENCHMARKS.glob("*/spec*.yaml"):
        if "schema_version" not in path.read_text(encoding="utf-8"):
            return False
    return True


@unittest.skipUnless(zero_five_corpus(), "the corpus is already in the 1.0 surface")
class CorpusTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory()
        cls.target = Path(cls.temporary.name) / "first"
        cls.result = convert([BENCHMARKS])
        write(cls.result, cls.target, [BENCHMARKS])

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def test_every_written_file_parses_and_keeps_its_comments(self):
        for path in sorted(BENCHMARKS.rglob("*.yaml")):
            relative = path.relative_to(ROOT)
            output = self.target / "benchmarks" / path.relative_to(BENCHMARKS)
            if not output.exists():
                self.assertIn(
                    str(relative.relative_to("benchmarks")),
                    "\n".join(self.result.removed),
                )
                continue
            text = output.read_text(encoding="utf-8")
            with self.subTest(file=str(relative)):
                yaml.safe_load(text)
                lost = Counter(
                    comment_texts(path.read_text(encoding="utf-8"))
                ) - Counter(comment_texts(text))
                self.assertEqual(lost, Counter())

    def test_the_follow_up_list_names_what_the_conversion_leaves_to_people(self):
        notes = {(note.file, note.kind) for note in self.result.notes}
        for item in (
            ("benchmarks/negative-version-mismatch/expected/error.yaml", "negative"),
            ("benchmarks/negative-lookup-unknown-key/expected/error.yaml", "negative"),
            (
                "benchmarks/negative-impute-missing-month/expected/error.yaml",
                "negative",
            ),
            (
                "benchmarks/negative-impute-unreachable-month/expected/error.yaml",
                "negative",
            ),
            ("benchmarks/negative-literal-structure/spec.yaml", "residue"),
            ("benchmarks/negative-literal-structure/expected/error.yaml", "negative"),
            ("benchmarks/negative-lookup-unknown-key/spec.yaml", "keys"),
            ("benchmarks/AGENTS.md", "D8"),
            ("benchmarks/schema-inheritance-clear/spec_study.yaml", "comment"),
        ):
            with self.subTest(note=item):
                self.assertIn(item, notes)
        kinds = Counter(note.kind for note in self.result.notes)
        self.assertNotIn("internal", kinds)
        self.assertNotIn("yaml", kinds)
        # Every benchmark that binds a codelist, an inventory vocabulary or a
        # project function runs with its project (CT-13).
        self.assertNotIn("project", kinds)
        self.assertEqual(kinds["residue"], 1)

    def test_a_second_run_changes_nothing(self):
        again = convert([self.target / "benchmarks"])
        target = Path(self.temporary.name) / "second"
        write(again, target, [self.target])
        changed = [
            str(path.relative_to(self.target))
            for path in sorted(self.target.rglob("*"))
            if path.is_file()
            and (target / path.relative_to(self.target)).read_bytes()
            != path.read_bytes()
        ]
        self.assertEqual(changed, [])
        self.assertEqual(again.removed, [])


if __name__ == "__main__":
    unittest.main()
