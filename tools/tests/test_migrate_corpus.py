"""The converter over the benchmarks still written in 0.5: properties that hold for any corpus.

Every written file parses and keeps its comments, the follow-up list names what
the conversion leaves to people, and a second run changes nothing. The corpus
moves to 1.0 one prefix at a time (#3030 P2-5 to P2-8), so the suite converts
only the benchmarks whose specifications are still written in 0.5, and runs
while any is (#3422). The counts the converter produced against the RFC's
measurement of main 1758ad145 were the acceptance check of #3165 and are
recorded on its pull request (#3186); they are a snapshot of a moving corpus,
so this suite does not pin them.
"""

import tempfile
import unittest
from collections import Counter
from pathlib import Path

import yaml
from tools.migrate import convert, write
from tools.migrate.text import comment_texts
from tools.paths import ROOT
from tools.surface import declares_header

BENCHMARKS = ROOT / "benchmarks"


def written_in_1_0(path):
    """Whether a specification declares the 1.x header; one that does not
    parse is read as 0.5, as the converter reads it."""
    try:
        return declares_header(yaml.safe_load(path.read_text(encoding="utf-8")))
    except yaml.YAMLError:
        return False


def zero_five_benchmarks():
    """The benchmarks with a specification still written in 0.5."""
    return [
        case
        for case in sorted(BENCHMARKS.iterdir())
        if (case / "benchmark.yaml").is_file()
        and not all(written_in_1_0(path) for path in case.glob("spec*.yaml"))
    ]


ZERO_FIVE = zero_five_benchmarks()


@unittest.skipUnless(ZERO_FIVE, "every benchmark is written in 1.0")
class CorpusTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory()
        cls.target = Path(cls.temporary.name) / "first"
        cls.result = convert(ZERO_FIVE, base=ROOT)
        write(cls.result, cls.target, ZERO_FIVE)

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def test_every_written_file_parses_and_keeps_its_comments(self):
        for path in sorted(p for case in ZERO_FIVE for p in case.rglob("*.yaml")):
            relative = path.relative_to(ROOT)
            output = self.target / relative
            if not output.exists():
                self.assertIn(str(relative), "\n".join(self.result.removed))
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
        # The expected notes name only the benchmarks still written in 0.5.
        if BENCHMARKS / "schema-inheritance-clear" in ZERO_FIVE:
            self.assertIn(
                ("benchmarks/schema-inheritance-clear/spec_study.yaml", "comment"),
                notes,
            )
        kinds = Counter(note.kind for note in self.result.notes)
        self.assertNotIn("internal", kinds)
        self.assertNotIn("yaml", kinds)
        # Every benchmark that binds a codelist, an inventory vocabulary or a
        # project function runs with its project (CT-13).
        self.assertNotIn("project", kinds)
        # The negatives whose fault a decided item changes are written in 1.0
        # or retired (P2-4: D60, D61, D137), so none is left for re-authoring
        # and no 0.5 spelling is left unconverted.
        self.assertNotIn("negative", kinds)
        self.assertNotIn("residue", kinds)

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
