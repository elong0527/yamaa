"""The converter over the whole benchmark corpus, with independent counts.

docs/design/interface-1.0.md and its method page measured the corpus at
main 1758ad145. The converter writes the current corpus into a temporary
directory and its counts are compared with independently authored totals.
Comments explain differences from the RFC, including later benchmarks.
When the corpus changes, count the affected constructs in the authored
sources before updating these controls; do not copy the converter's totals.
The RFC's measurement scripts were not committed, so measurement differences
are reported rather than forced.
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

# (area, row): (occurrences, documents). "files" are YAML files, "markdown"
# the YAML blocks of benchmarks/*/README.md, README.md and AGENTS.md.
FOOTPRINT = {
    # RFC 484: negative-format-name-without-dollar adds a spec and project;
    # schema-linked-csv-types adds two specs.
    ("files", "header (D4)"): (488, 488),
    ("files", "literal (D18)"): (891, 167),
    ("files", "{literal: null} in value fields (D18)"): (156, 95),
    ("files", "intermediate no_match (P-HND-1, P-HND-3)"): (97, 68),
    # RFC 145 / 28: negative-format-name-without-dollar adds SEX's origin.
    ("files", "origins (P-ORIGIN-1)"): (146, 29),
    # Not an RFC row: the RFC counts column-level origins only, and these
    # value-level ones convert the same way.
    ("files", "value-level origins (P-ORIGIN-1)"): (5, 3),
    ("files", "compute (D18)"): (84, 55),
    ("files", "intermediates without key (D23)"): (31, 27),
    ("files", "project environments (D8)"): (8, 8),
    ("files", "environment functions (D8)"): (16, 16),
    ("files", "not_missing: {} (P-HND-4)"): (58, 38),
    ("files", "rows[].derivations: {} (P-HND-4)"): (9, 9),
    ("files", "baseline_flag operands (P-OP-2, P-OP-3)"): (18, 9),
    ("files", "section standard (D16)"): (17, 17),
    ("files", "minimum_source_precision (P-IMP-1)"): (15, 11),
    ("files", "{source: X} in wrappers (D18)"): (10, 9),
    ("files", "aggregate derive (P-BIND-1)"): (10, 7),
    ("files", "study_day.date (P-OP-1)"): (14, 11),
    ("files", "datasets (P-PRJ-1)"): (14, 14),
    # RFC 17 / 14: schema-linked-csv-types adds the inclusive TRTDURD diff.
    ("files", "diff.bounds and cut.right (P-ORD-2)"): (18, 15),
    ("files", "tie_break (P-ORD-1)"): (2, 2),
    # Not an RFC row: the three tagged arguments of schema-non-finite.
    ("files", "tagged function arguments (D18)"): (3, 1),
    ("markdown", "header (D4)"): (1, 1),
    # RFC 9 / 5: the converter also counts the `{literal: null}` row
    # derivation of negative-lookup-key-other-template's README.
    ("markdown", "literal (D18)"): (10, 6),
    ("markdown", "{literal: null} in value fields (D18)"): (13, 11),
    ("markdown", "intermediate no_match (P-HND-1, P-HND-3)"): (7, 7),
    ("markdown", "origins (P-ORIGIN-1)"): (4, 4),
    ("markdown", "compute (D18)"): (5, 5),
    # RFC 16 / 16: the README blocks hold 7 intermediates without `key`;
    # the other {id, dataset} lists are row templates, which declare none.
    ("markdown", "intermediates without key (D23)"): (7, 7),
    ("markdown", "not_missing: {} (P-HND-4)"): (2, 2),
    ("markdown", "rows[].derivations: {} (P-HND-4)"): (1, 1),
    ("markdown", "baseline_flag operands (P-OP-2, P-OP-3)"): (6, 3),
    ("markdown", "minimum_source_precision (P-IMP-1)"): (5, 5),
    ("markdown", "{source: X} in wrappers (D18)"): (2, 2),
    ("markdown", "study_day.date (P-OP-1)"): (3, 3),
    ("markdown", "datasets (P-PRJ-1)"): (1, 1),
    ("markdown", "tie_break (P-ORD-1)"): (2, 2),
}

# The RFC's "any change" row: 484 / 484 files and 59 blocks in 52 README
# files. The four new files counted in "header (D4)" also change. The converter
# changes 52 blocks in 50 files: it rewrites only the blocks above and leaves
# the project block of benchmarks/AGENTS.md, whose environment files it cannot
# merge, for manual work.
DOCUMENTS_CHANGED = {"files": 488, "markdown": 52}

STATISTICS = {
    # RFC: 415 entry specifications with a model and 10 without. The RFC
    # counted the benchmark entries it could compose; the converter also
    # writes `model` on negative-remote-parent's entry and on the
    # specification sdtm-lb-metadata's submission project lists, and finds
    # no model source for negative-cyclic-parent. Later benchmarks add
    # negative-format-name-without-dollar's entry and both entries of
    # schema-linked-csv-types.
    "entry specifications with a model (P-MODEL-1)": 420,
    "entry specifications without a model source (P-MODEL-1)": 11,
    "environment files removed (D8)": 16,
    "function files written (D8)": 8,
    # RFC: 26 intermediate keys; it counts negative-lookup-unknown-key's,
    # which no input supplies (listed for manual repair, D60).
    "intermediate keys (D23)": 25,
    "intermediate row-template drivers left keyless (D23-R1)": 5,
    # RFC: 86 input declarations, 59 join pairs (8 on rows another
    # template's driver built), 32 aggregate inputs, 5 both. The converter
    # follows the engine's bindings: a read in a lookup's own fields (a key
    # match value, `between.value`, an intermediate filter, a named window)
    # is never a join, and a column derivation joins only on the rows of the
    # templates that leave the column to the column phase (#2829). The RFC
    # walker also counted those reads, which the four negatives among them
    # refuse as unknown_field.
    "input key declarations (P-JOIN-1)": 76,
    "join pairs (P-JOIN-1)": 49,
    "join pairs on rows another template's driver built (NAME-4)": 3,
    "inputs of keyless qualified aggregates (P-KEY-1)": 32,
    "inputs both joined and aggregated": 5,
    # RFC: 55 keyless qualified aggregates; the converter also declares the
    # input of negative-keys-aggregate's aggregate over its own base.
    "keyless qualified aggregates (P-KEY-1)": 56,
    # RFC: 92 specifications, 24 of them negative (the join pairs above).
    "specifications with keys to write (D23)": 84,
    "negative specifications with keys to write (D23)": 19,
    # The method page: 196 spec paths in the 189 negative contracts, 37 of
    # them changed by P-CLR-2 and 12 by D18 and the renames; plus clear[0].
    # negative-format-name-without-dollar adds one unchanged format_name path.
    "negative spec paths": 197,
    "negative spec paths rewritten": 49,
    "negative contracts rewritten": 47,
    # P-COPY-1: 10 benchmark columns in 4 specifications, plus the renamed
    # TRTSDT and TRTEDT copies in schema-linked-csv-types/spec_adsl.yaml.
    "copy candidates (P-COPY-1)": 12,
    "Markdown files with converted blocks": 50,
}

KEYS = {
    "join key ['STUDYID', 'USUBJID']": 43,
    "join key ['USUBJID']": 2,
    "join key ['STUDYID', 'USUBJID', 'AESEQ']": 1,
    "NAME-4 join key ['STUDYID', 'USUBJID']": 1,
    "NAME-4 join key ['DOMAIN', 'USUBJID']": 2,
    "aggregate key ['STUDYID', 'USUBJID']": 38,
    "aggregate key ['STUDYID', 'USUBJID', 'EXTRT']": 8,
    # RFC: 7.
    "aggregate key ['USUBJID']": 8,
    "aggregate key ['STUDYID', 'USUBJID', 'AVISIT']": 2,
}


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

    def test_the_footprint_matches_the_rfc(self):
        for key, (occurrences, documents) in FOOTPRINT.items():
            with self.subTest(row=key):
                self.assertEqual(
                    (self.result.counts[key], len(self.result.documents[key])),
                    (occurrences, documents),
                )
        for area, documents in DOCUMENTS_CHANGED.items():
            with self.subTest(area=area):
                self.assertEqual(
                    len(self.result.documents[(area, "any change")]), documents
                )

    def test_keys_models_and_negative_paths_match_the_rfc(self):
        for name, value in {**STATISTICS, **KEYS}.items():
            with self.subTest(statistic=name):
                self.assertEqual(self.result.stats[name], value)
        keys = {name for name in self.result.stats if " key [" in name}
        self.assertEqual(keys, set(KEYS))

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

    def test_the_follow_up_list_names_what_the_rfc_leaves_to_people(self):
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
            ("benchmarks/sdtm-lb-metadata/spec.yaml", "project"),
            ("benchmarks/schema-inheritance-clear/spec_study.yaml", "comment"),
        ):
            with self.subTest(note=item):
                self.assertIn(item, notes)
        kinds = Counter(note.kind for note in self.result.notes)
        self.assertNotIn("internal", kinds)
        self.assertNotIn("yaml", kinds)
        self.assertEqual(kinds["model"], 11)
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
