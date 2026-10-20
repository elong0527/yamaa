"""Each tool reads a benchmark and its 1.0 conversion alike (#3030 P2-3).

The converter (`tools.migrate`) writes the whole corpus as 1.0 into a
temporary directory, as `make check-conversion` does, and each tool that reads
benchmark specifications and metadata runs on both trees:

- the conformance planner plans the same entries, exclusions, command lines
  and metamorphic variants, each variant written in its benchmark's language;
- the dashboard renders the same page, apart from the text of the files the
  conversion rewrites (specifications, READMEs, raw YAML fixtures);
- vocabulary coverage accepts the same declared negatives.

A benchmark already in 1.0 converts to itself, so this holds while the corpus
moves one prefix at a time (P2-5 to P2-8). The benchmark runner and the
installed conformance runs are compared on the same conversion by
`make check-conversion`.
"""

import json
import re
import tempfile
import unittest
from contextlib import ExitStack
from pathlib import Path
from unittest import mock

from tools.benchmark import directories, load_metadata
from tools.benchmark_docs import generate
from tools.commands import conversion
from tools.conformance import public_conformance
from tools.migrate.excepted import EXCEPTED
from tools.validation import check_vocabulary_coverage

# Dashboards the conversion changes by decision, beside the excepted
# benchmarks, whose conversion changes their meaning until P2-4, P2-7 and
# P1-11 land (tools/migrate/excepted.py).
DASHBOARD_DIFFERENCES = {
    # P-CLR-1: a 0.5 child layer writes a cleared field as a column entry
    # holding null, which the sheet lists as a row; the 1.0 child lists the
    # path in `clear` and declares no column entry.
    "negative-property-clear": "P-CLR-1",
    "schema-inheritance-clear": "P-CLR-1",
    # D18: `{compute: {expr: TRORRES_N}}` is the reference `TRORRES_N` in 1.0,
    # which a case result renders as the 0.5 reference it is.
    "sdtm-tr-tumor-measurements": "D18",
}
# D23: an intermediate that omits `key` in 0.5 declares the inferred keys in
# 1.0, so the sheet names them where the 0.5 sheet says "the shared output
# keys"; the comparison reads both as one phrase.
JOIN_KEYS = re.compile(
    r"matching on (?:the shared output keys"
    r"|[A-Z_][A-Z0-9_]*(?:, [A-Z_][A-Z0-9_]*)*(?: and [A-Z_][A-Z0-9_]*)?)"
)
PLAIN_FILE = re.compile(r'<pre class="plain-file"><code>.*?</code></pre>', re.S)
HEADER = re.compile(r'(?:schema_version: "1\.0"|yamaa: "1\.0"\ntype: spec)\n')


def without_header(text):
    """A specification's text without the header that names its language."""
    return HEADER.sub("", text, count=1)


class BothSurfaces(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory()
        cls.converted = conversion.convert(Path(cls.temporary.name) / "converted")
        cls.cases = [case.name for case in directories(conversion.FIXTURES)]

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def test_the_conversion_holds_every_benchmark(self):
        self.assertEqual(
            [case.name for case in directories(self.converted)], self.cases
        )

    def test_the_planner_plans_both_surfaces_alike(self):
        original = public_conformance.plan(conversion.FIXTURES)
        converted = public_conformance.plan(self.converted)
        self.assertEqual(original, converted)
        for example in original["examples"]:
            name = example["name"]
            _, before = public_conformance.example_variants(
                conversion.FIXTURES, example
            )
            _, after = public_conformance.example_variants(self.converted, example)
            self.assertEqual(set(before), set(after), name)
            for path, text in before.items():
                with self.subTest(benchmark=name, variant=path):
                    if "__keyorder_" in path:
                        # The texts are the document in its own language.
                        self.assertEqual(
                            [c["name"] for c in json.loads(text)["columns"]],
                            [c["name"] for c in json.loads(after[path])["columns"]],
                        )
                    else:
                        # An empty child declares its parent's header.
                        self.assertEqual(
                            without_header(text), without_header(after[path])
                        )

    def test_the_dashboard_renders_both_surfaces_alike(self):
        readme = generate.render_readme
        with ExitStack() as patches:
            # The text of a file the conversion rewrites is shown as written.
            for name, replacement in (
                (
                    "render_spec_pane",
                    lambda filename, text, slug, single, edit_url=None: (
                        ("<spec>", 0) if single else (f"<spec {filename}>", 0)
                    ),
                ),
                ("render_readme", lambda text, url: (readme(text, url)[0], "")),
                ("readme_body_line_count", lambda text: 0),
            ):
                patches.enter_context(mock.patch.object(generate, name, replacement))
            differing = []
            for name in self.cases:
                pages = [
                    JOIN_KEYS.sub(
                        "matching on <keys>",
                        PLAIN_FILE.sub(
                            "<file>",
                            generate.render_benchmark(root / name).decode("ascii"),
                        ),
                    )
                    for root in (conversion.FIXTURES, self.converted)
                ]
                if pages[0] != pages[1]:
                    differing.append(name)
        self.assertLessEqual(set(differing), set(DASHBOARD_DIFFERENCES) | set(EXCEPTED))

    def test_vocabulary_coverage_accepts_both_surfaces_alike(self):
        declared = check_vocabulary_coverage.declared_coverage()
        for root in (conversion.FIXTURES, self.converted):
            with mock.patch.object(check_vocabulary_coverage, "EXAMPLES", root):
                for operation, owners in sorted(declared.items()):
                    names = [
                        name for name in owners["benchmarks"] if name not in EXCEPTED
                    ]
                    if names:
                        with self.subTest(operation=operation, root=root.name):
                            self.assertEqual(
                                check_vocabulary_coverage.check(operation, names), []
                            )

    def test_the_metadata_is_never_rewritten(self):
        for name in self.cases:
            self.assertEqual(
                load_metadata(conversion.FIXTURES / name),
                load_metadata(self.converted / name),
                name,
            )


if __name__ == "__main__":
    unittest.main()
