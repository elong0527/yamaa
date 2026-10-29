"""The repository's lint command over the installed engine, and the style
guide naming every finding the lint reports."""

import contextlib
import io
import os
import tempfile
import unittest
from pathlib import Path

import yaml
from tools.commands import lint
from tools.paths import ROOT

CANONICAL = """\
yamaa: "1.0"
type: spec
domain: DM
keys: [USUBJID]
input:
  DM: input/dm.csv

output:
  path: dm.csv
  columns: [USUBJID]

columns:
  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: DM.USUBJID
"""


class LintCommand(unittest.TestCase):
    def run_command(self, *arguments):
        stdout, stderr = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            status = lint.main([str(argument) for argument in arguments])
        return status, stdout.getvalue(), stderr.getvalue()

    def test_the_exit_status_follows_the_findings(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            clean = root / "clean.yaml"
            clean.write_text(CANONICAL)
            packed = root / "packed.yaml"
            packed.write_text(CANONICAL.replace("dm.csv\n\noutput:", "dm.csv\noutput:"))
            broken = root / "broken.yaml"
            broken.write_text("a: [\n")
            status, stdout, stderr = self.run_command(clean)
            self.assertEqual((status, stdout), (0, ""))
            self.assertIn("0 lint findings in 0 of 1 file", stderr)
            status, stdout, _ = self.run_command(clean, packed)
            self.assertEqual(status, 1)
            absolute = os.path.abspath(packed).replace(os.sep, "/")
            self.assertEqual(
                stdout,
                f"{absolute}:7:1: root_spacing leave one blank line before `output`\n",
            )
            status, _, stderr = self.run_command(packed, broken)
            self.assertEqual(status, 2)
            self.assertIn("cannot lint", stderr)
            self.assertIn("invalid_yaml", stderr)
            self.assertEqual(self.run_command("--fix", packed)[0], 0)
            self.assertEqual(packed.read_text(), CANONICAL)

    def test_the_style_guide_names_every_finding(self):
        definition = yaml.safe_load(
            (ROOT / "rust/interface/bindings.yaml").read_text(encoding="utf-8")
        )
        guide = (ROOT / "docs/articles/style.md").read_text(encoding="ascii")
        for name in definition["values"]["lint_finding"]["values"]:
            with self.subTest(finding=name):
                self.assertIn(f"`{name}`", guide)


if __name__ == "__main__":
    unittest.main()
