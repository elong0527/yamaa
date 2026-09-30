import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
module_spec = importlib.util.spec_from_file_location(
    "opencode_eval", HERE / "opencode_eval.py"
)
opencode_eval = importlib.util.module_from_spec(module_spec)
sys.modules["opencode_eval"] = opencode_eval
module_spec.loader.exec_module(opencode_eval)


class ReadmeTest(unittest.TestCase):
    def test_how_to_fix_and_badges_never_reach_a_section(self):
        readme = opencode_eval.parse_readme(
            "# Reject Something\n\n"
            "[![Dashboard](https://example.test/badge)](https://example.test)\n\n"
            "**Goal:** attempt one record\nper subject.\n\n"
            "**Variables:**\n\n- `A`: the first\n  value.\n- `B`: the second.\n\n"
            "**Standard:** ADaM | **Domain:** ADSL\n\n"
            "## How to fix\n\nRename the window.\n"
        )
        self.assertEqual(readme.title, "Reject Something")
        self.assertEqual((readme.standard, readme.domain), ("ADaM", "ADSL"))
        self.assertEqual(readme.section("Goal"), "Attempt one record per subject.")
        self.assertEqual(
            readme.section("Variables"), "- `A`: the first value.\n- `B`: the second."
        )
        text = "\n".join(body for _, body in readme.sections)
        self.assertNotIn("Rename the window", text)
        self.assertNotIn("badge", text)

    def test_a_pointer_at_a_committed_answer_is_dropped(self):
        self.assertEqual(
            opencode_eval._without_answer_paths(
                "Reads `input/a.csv`; the expected result is `expected/a.csv`. "
                "The chain resolves to `expected/spec_resolved.yaml`. Keep this."
            ),
            "Reads `input/a.csv`. Keep this.",
        )


class TaskSetTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.scratch = tempfile.TemporaryDirectory()
        cls.out = Path(cls.scratch.name)
        cls.tasks = opencode_eval.generate(cls.out)

    @classmethod
    def tearDownClass(cls):
        cls.scratch.cleanup()

    def test_every_benchmark_is_exactly_one_task(self):
        self.assertEqual(
            sorted(task.case for task in self.tasks),
            [case.name for case in opencode_eval.benchmark_cases()],
        )
        self.assertEqual(opencode_eval.verify(self.out), [])

    def test_an_author_task_withholds_its_entry_specification(self):
        task = next(task for task in self.tasks if task.case == "adam-adae-severity")
        self.assertEqual(task.kind, "positive")
        self.assertEqual(task.given, ["input/ae.csv", "input/supp.csv"])
        prompt = (self.out / "tasks" / task.case / "PROMPT.md").read_text("utf-8")
        self.assertIn(
            "`output/adae.csv`: STUDYID, USUBJID, AESEQ, AEDECOD, AESEV", prompt
        )

    def test_a_negative_task_hands_over_its_specification_but_not_its_fix(self):
        task = next(task for task in self.tasks if task.case == "negative-adeg-rrr")
        self.assertEqual(task.kind, "negative")
        self.assertIn("spec.yaml", task.given)
        prompt = (self.out / "tasks" / task.case / "PROMPT.md").read_text("utf-8")
        self.assertTrue(prompt.startswith("# Run the ADEG specification"))
        self.assertNotIn("Reject", prompt)


if __name__ == "__main__":
    unittest.main()
