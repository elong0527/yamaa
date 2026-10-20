"""The 0.5-to-1.0 converter (tools.migrate): the hand-converted examples and each transform.

The six benchmarks the RFC converted by hand (docs/design/interface-1.0/
examples/) are the acceptance: the converter writes the same YAML values and
keeps every comment of the source, and a second run changes nothing. The
other tests pin each transform of the rename table on a small document.
"""

import subprocess
import sys
import tempfile
import unittest
from collections import Counter
from pathlib import Path

import yaml
from tools.migrate import SourceError, convert, write
from tools.migrate import table as table_module
from tools.migrate.convert import Converter, Plan
from tools.migrate.text import comment_texts
from tools.paths import ROOT

EXAMPLES = ROOT / "docs/design/interface-1.0/examples"
BENCHMARKS = ROOT / "benchmarks"
CONVERTED = (
    "adam-adsl-bmi",
    "adam-adtte-pfs",
    "schema-inheritance",
    "schema-inheritance-clear",
    "sdtm-dm-metadata",
    "sdtm-lb-multiform",
)


def run(text, **plan):
    """Convert one document under a plan; return the text and the converter."""
    plan.setdefault("label", "test.yaml")
    converter = Converter(Plan(**plan), notes=[])
    return converter.convert(text), converter


def tree(root, files):
    for name, text in files.items():
        path = Path(root) / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
    return Path(root)


def converted(files, *sources):
    """Convert a temporary benchmark built from `files`; return (result, its texts)."""
    with tempfile.TemporaryDirectory() as directory:
        case = tree(Path(directory) / "case", files)
        result = convert([case / source for source in sources] or [case])
    texts = {
        name.removeprefix("case/"): content
        for name, content in result.files.items()
        if isinstance(content, str)
    }
    return result, texts


class ExampleTests(unittest.TestCase):
    """The acceptance of #3165 on the six hand-converted benchmarks."""

    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory()
        cls.target = Path(cls.temporary.name) / "first"
        cls.result = convert([BENCHMARKS / name for name in CONVERTED])
        write(cls.result, cls.target, [BENCHMARKS])

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def test_every_example_file_has_the_same_yaml_values(self):
        examples = sorted(EXAMPLES.rglob("*.yaml"))
        self.assertEqual(len(examples), 12)
        for example in examples:
            relative = example.relative_to(EXAMPLES)
            with self.subTest(file=str(relative)):
                output = self.target / relative
                self.assertTrue(output.is_file(), f"{relative} was not written")
                self.assertEqual(
                    yaml.safe_load(output.read_text(encoding="utf-8")),
                    yaml.safe_load(example.read_text(encoding="utf-8")),
                )

    def test_every_comment_of_the_source_survives(self):
        for name in CONVERTED:
            for source in sorted((BENCHMARKS / name).rglob("*.yaml")):
                relative = source.relative_to(BENCHMARKS)
                output = self.target / relative
                if source.parent.name in ("python", "r") and not output.exists():
                    # A merged environment file: its comments go to the
                    # function files beside the project file.
                    output_texts = [
                        path.read_text(encoding="utf-8")
                        for path in (self.target / name / "functions").glob("*.yaml")
                    ]
                else:
                    output_texts = [output.read_text(encoding="utf-8")]
                kept = Counter()
                for text in output_texts:
                    kept.update(comment_texts(text))
                with self.subTest(file=str(relative)):
                    lost = (
                        Counter(comment_texts(source.read_text(encoding="utf-8")))
                        - kept
                    )
                    self.assertEqual(lost, Counter())

    def test_the_environment_files_merge_into_one_function_file(self):
        self.assertEqual(
            sorted(self.result.removed),
            [
                "adam-adsl-bmi/python/environment.yaml",
                "adam-adsl-bmi/r/environment.yaml",
            ],
        )
        self.assertFalse(
            (self.target / "adam-adsl-bmi/python/environment.yaml").exists()
        )
        self.assertTrue((self.target / "adam-adsl-bmi/r/renv.lock").is_file())

    def test_a_second_run_changes_nothing(self):
        again = convert([self.target / name for name in CONVERTED])
        target = Path(self.temporary.name) / "second"
        write(again, target, [self.target])
        for path in sorted(self.target.rglob("*")):
            if path.is_file():
                with self.subTest(file=str(path.relative_to(self.target))):
                    self.assertEqual(
                        (target / path.relative_to(self.target)).read_bytes(),
                        path.read_bytes(),
                    )
        self.assertEqual(again.removed, [])

    def test_only_the_moved_clear_comment_needs_review(self):
        self.assertEqual(
            [(note.file, note.kind) for note in self.result.notes],
            [("schema-inheritance-clear/spec_study.yaml", "comment")] * 2,
        )


class HeaderTests(unittest.TestCase):
    def test_header_writes_the_version_quoted_type_and_model(self):
        text, _ = run("# leading\nschema_version: '1.0'\ndomain: DM\n", model="SDTM")
        self.assertEqual(
            text, '# leading\nyamaa: "1.0"\ntype: spec\nmodel: SDTM\ndomain: DM\n'
        )

    def test_project_header_names_the_project_type(self):
        text, _ = run(
            "schema_version: '1.0'\nsdtm:\n  specs: [dm.yaml]\n",
            bundle="project",
            header_type="project",
        )
        self.assertTrue(text.startswith('yamaa: "1.0"\ntype: project\n'))


class ExpressionTests(unittest.TestCase):
    """D18: literal, compute and the string form of source merge into the grammar."""

    def test_literals_become_quoted_text_or_yaml_scalars(self):
        source = (
            "schema_version: '1.0'\ncolumns:\n"
            "  - name: A\n    derivation: {literal: DM}\n"
            '  - name: B\n    derivation: {literal: "it\'s"}\n'
            "  - name: C\n    derivation:\n      case:\n"
            '        - when: "X > 1"\n          then: {literal: 0}\n'
            '        - when: "X > 2"\n          then: {literal: true}\n'
            "        - otherwise: {literal: null}\n"
            '  - name: D\n    derivation: {literal: "1"}\n'
        )
        text, _ = run(source)
        self.assertIn("derivation: \"'DM'\"", text)
        self.assertIn("derivation: \"'it''s'\"", text)
        self.assertIn("then: 0\n", text)
        self.assertIn("then: true\n", text)
        self.assertIn("otherwise: null\n", text)
        self.assertIn("derivation: \"'1'\"", text)

    def test_compute_becomes_its_expression_and_a_comment_inside_moves_above(self):
        source = (
            "schema_version: '1.0'\ncolumns:\n  - name: A\n    derivation:\n"
            '      compute:  # the ratio\n        expr: "X / Y"\n'
        )
        text, converter = run(source)
        self.assertIn('    # the ratio\n    derivation: "X / Y"\n', text)
        self.assertIn("comment", {note.kind for note in converter.notes})

    def test_source_wrapper_becomes_the_name_and_a_binding_stays(self):
        source = (
            "schema_version: '1.0'\ncolumns:\n"
            "  - name: A\n    derivation:\n      value: {source: LBORRES}\n"
            "      unconvertible: {literal: null}\n"
            "  - name: B\n    derivation:\n      source: {variable: DM.AGE, absent: 0}\n"
            "intermediates:\n  - id: R\n    dataset: X\n    key: {K: {source: A}}\n"
        )
        text, _ = run(source)
        self.assertIn("value: LBORRES\n      unconvertible: null\n", text)
        self.assertIn("source: {variable: DM.AGE, absent: 0}", text)
        self.assertIn("key: {K: A}", text)

    def test_function_arguments_take_quoted_text_and_temporal_literals(self):
        source = (
            "schema_version: '1.0'\ncolumns:\n  - name: A\n    derivation:\n"
            "      function:\n        name: f\n        args:\n"
            "          kind: {literal: nan}\n          when: {date: 2024-01-31}\n"
            "          at: {datetime: '2024-01-31T10:00:00'}\n"
            "          gone: {literal: null}\n          ref: WEIGHT\n"
        )
        text, _ = run(source)
        self.assertIn("kind: \"'nan'\"", text)
        self.assertIn("when: \"DATE '2024-01-31'\"", text)
        self.assertIn("at: \"DATETIME '2024-01-31T10:00:00'\"", text)
        self.assertIn("gone: null", text)
        self.assertIn("ref: WEIGHT", text)

    def test_untyped_intermediate_value_is_unwrapped(self):
        source = (
            "schema_version: '1.0'\nintermediates:\n  - id: I\n    dataset: X\n"
            "    key: [K]\n    derivations:\n      D: {value: {literal: 1}}\n"
        )
        text, _ = run(source)
        self.assertIn("      D: 1\n", text)


class HandlerTests(unittest.TestCase):
    """YAML null for {literal: null}, restated defaults (P-HND-3) and P-HND-1."""

    SOURCE = (
        "schema_version: '1.0'\n{parent}"
        "intermediates:\n  - id: I\n    dataset: X\n    key: [K]\n"
        "    no_match: {{literal: null}}\n"
        "  - id: J\n    dataset: X\n    key: [K]\n    no_match: OTHER\n"
        "columns:\n  - name: A\n    derivation:\n      mapping:\n"
        "        source: X\n        dict: {{P: {{literal: null}}}}\n"
        "        missing: {{literal: null}}\n        unmapped: {{literal: null}}\n"
        '  - name: F\n    derivation:\n      flag:\n        condition: "X > 1"\n'
        "        false_value: N\n        missing: {{literal: null}}\n"
    )

    def test_a_single_layer_drops_restated_defaults_and_renames_no_match(self):
        text, _ = run(self.SOURCE.format(parent=""))
        self.assertNotIn("no_match", text)
        self.assertIn("    absent: OTHER\n", text)
        self.assertIn("dict: {P: null}", text)
        self.assertNotIn("        missing:", text.split("flag:")[0])
        self.assertIn("unmapped: null", text)
        self.assertIn("false_value: N\n        missing: null\n", text)

    def test_a_child_layer_keeps_each_handler_as_null(self):
        text, _ = run(self.SOURCE.format(parent="parent: base.yaml\n"), layered=True)
        self.assertIn("    absent: null\n", text)
        self.assertIn("        missing: null\n        unmapped: null\n", text)

    def test_a_yaml_null_in_a_single_layer_is_deleted_with_its_comment_kept(self):
        source = (
            "schema_version: '1.0'\nintermediates:\n  - id: I\n    dataset: X\n"
            "    key: [K]\n    no_match: null  # absent\n"
        )
        text, converter = run(source)
        self.assertEqual(
            text,
            'yamaa: "1.0"\ntype: spec\nintermediates:\n  - id: I\n    dataset: X\n'
            "    key: [K]\n    # absent\n",
        )
        self.assertTrue(converter.markers)


class StructureTests(unittest.TestCase):
    def test_origin_becomes_a_list_in_the_file_sequence_style(self):
        indented = (
            "schema_version: '1.0'\nkeys: [A]\ncolumns:\n  - name: A\n    submission:\n"
            "      origin:\n        type: Assigned\n        source: Sponsor\n"
            "      value_level:\n        - id: V\n          where: {A: [X]}\n"
            "          origin: {type: Derived}\n"
        )
        text, _ = run(indented)
        self.assertIn(
            "      origins:\n        - type: Assigned\n          source: Sponsor\n",
            text,
        )
        self.assertIn("origins: [{type: Derived}]", text)
        indentless = (
            "schema_version: '1.0'\nkeys: [A]\ncolumns:\n- name: A\n  submission:\n"
            "    origin:\n      type: Assigned\n      source: Sponsor\n"
            "    value_level:\n    - id: V\n      where: {A: [X]}\n"
        )
        text, _ = run(indentless)
        self.assertIn(
            "    origins:\n    - type: Assigned\n      source: Sponsor\n", text
        )

    def test_section_standard_becomes_standards_and_datasets_transport(self):
        source = (
            "schema_version: '1.0'\nsdtm:\n  specs:\n  - dm.yaml\n"
            "  standard:\n    name: SDTMIG\n    version: '3.4'\n    status: Final\n"
            "  datasets: json\nadam:\n  specs: [adsl.yaml]\n"
            "  standard: {name: ADaMIG, version: '1.3', status: Final}\n  datasets: xpt\n"
        )
        text, _ = run(source, bundle="project", header_type="project")
        self.assertIn("  standards:\n  - name: SDTMIG\n    version: '3.4'\n", text)
        self.assertIn("  transport: dataset_json\n", text)
        self.assertIn(
            "standards: [{name: ADaMIG, version: '1.3', status: Final}]", text
        )
        self.assertIn("  transport: xpt\n", text)

    def test_derive_becomes_a_derivations_map(self):
        source = (
            "schema_version: '1.0'\ncolumns:\n  - name: A\n    derivation:\n"
            "      aggregate:\n        key: [K]\n        derive:\n"
            "          - name: N\n            type: float\n"
            "            derivation: {literal: 1}\n"
            "          # second\n"
            "          - type: int\n            name: M\n            derivation: X.Y\n"
            '        expr: "SUM(N)"\n'
            "  - name: B\n    derivation:\n      aggregate:\n        key: [K]\n"
            "        derive: [{name: Q, type: int, derivation: X.Z}]\n"
            '        expr: "SUM(Q)"\n'
        )
        text, _ = run(source)
        self.assertIn(
            "        derivations:\n          N:\n            type: float\n"
            "            derivation: 1\n          # second\n          M:\n"
            "            type: int\n            derivation: X.Y\n",
            text,
        )
        self.assertIn("derivations: {Q: {type: int, derivation: X.Z}}", text)

    def test_tie_break_folds_into_the_window_order(self):
        source = (
            "schema_version: '1.0'\ncolumns:\n"
            "  - name: A\n    derivation:\n      baseline_flag:\n        date: ADT\n"
            "        reference_date: TRTSDT\n        value: AVAL\n"
            "        tie_break: [ATM, LBSEQ]\n        window:\n          group_by: [USUBJID]\n"
            "  - name: B\n    derivation:\n      baseline_flag:\n        date: ADT\n"
            "        reference_date: TRTSDT\n        value: AVAL\n        tie_break: [ATM]\n"
        )
        text, _ = run(source)
        self.assertIn(
            "        reference: TRTSDT\n        source: AVAL\n        window:\n"
            "          group_by: [USUBJID]\n          order_by: [ADT, ATM, LBSEQ]\n",
            text,
        )
        self.assertIn("        window: {order_by: [ADT, ATM]}\n", text)
        self.assertNotIn("tie_break", text)

    def test_a_named_window_beside_tie_break_is_listed(self):
        source = (
            "schema_version: '1.0'\ncolumns:\n  - name: A\n    derivation:\n"
            "      baseline_flag:\n        date: ADT\n        reference_date: R\n"
            "        value: V\n        tie_break: [ATM]\n        window: W\n"
        )
        text, converter = run(source)
        self.assertIn("tie_break: [ATM]", text)
        self.assertIn("tie_break", {note.kind for note in converter.notes})

    def test_endpoints_become_closed(self):
        source = (
            "schema_version: '1.0'\ncolumns:\n"
            "  - name: A\n    derivation:\n      diff: {type: date, start: S, end: E, unit: day, bounds: inclusive}\n"
            "  - name: B\n    derivation:\n      diff: {type: date, start: S, end: E, unit: day, bounds: between}\n"
            "  - name: C\n    derivation:\n      cut:\n        source: X\n        breaks: [1]\n"
            "        labels: [a, b]\n        right: true\n"
            "  - name: D\n    derivation:\n      cut:\n        source: X\n        breaks: [1]\n"
            "        labels: [a, b]\n        right: false\n"
        )
        text, _ = run(source)
        self.assertIn("closed: both}", text)
        self.assertIn("closed: neither}", text)
        self.assertIn("        closed: right\n", text)
        self.assertEqual(text.count("right"), 1)

    def test_operands_and_impute_follow_the_table(self):
        source = (
            "schema_version: '1.0'\ncolumns:\n"
            "  - name: A\n    derivation:\n      study_day: {date: ADT, reference: TRTSDT}\n"
            "  - name: B\n    derivation:\n      impute:\n        type: date\n        source: X\n"
            "        month: 6\n        day: 15\n        minimum_source_precision: year\n"
        )
        text, _ = run(source)
        self.assertIn("study_day: {source: ADT, reference: TRTSDT}", text)
        self.assertNotIn("minimum_source_precision", text)

    def test_empty_mappings_drop(self):
        source = (
            "schema_version: '1.0'\ncolumns:\n  - name: A\n    verifications:\n"
            "      - not_missing: {}\n      - not_missing: {severity: warning}\n"
            "rows:\n  - id: r\n    derivations: {}\n"
        )
        text, _ = run(source)
        self.assertIn(
            "      - not_missing\n      - not_missing: {severity: warning}\n", text
        )
        self.assertNotIn("derivations", text)

    def test_suppression_comments_name_lint(self):
        text, _ = run(
            "schema_version: '1.0'\n# yamaa-style: allow line_width -- table\ndomain: DM\n"
        )
        self.assertIn("# yamaa-lint: allow line_width -- table\n", text)

    def test_a_structured_literal_is_left_and_listed(self):
        source = "schema_version: '1.0'\ncolumns:\n  - name: A\n    derivation: {literal: {a: 1}}\n"
        text, converter = run(source)
        self.assertIn("{literal: {a: 1}}", text)
        self.assertEqual(["residue"], [note.kind for note in converter.notes])


class LayerTests(unittest.TestCase):
    """P-CLR-1: a child layer's YAML null becomes a clear entry."""

    def test_an_inherited_value_is_cleared_and_an_emptied_entry_dropped(self):
        parent = (
            "schema_version: '1.0'\ndomain: ADSL\nkeys: [USUBJID]\n"
            "input:\n  DM: {path: dm.csv, require_match: true}\n"
            "columns:\n  - name: USUBJID\n    type: str\n    label: Subject\n"
            "  - name: RACEN\n    type: int\n    derivation:\n      mapping:\n"
            "        source: DM.RACE\n        dict: {WHITE: 1, NOT REPORTED: 9}\n"
        )
        child = (
            "schema_version: '1.0'\nparent: parent.yaml\n"
            "input:\n  DM: {require_match: null}\n\n"
            "columns:\n  # drops the code\n  - name: RACEN\n    derivation:\n"
            "      mapping:\n        dict:\n          NOT REPORTED: null\n"
            "  - name: USUBJID\n    label: null\n    metadata: {note: null}\n"
            "\noutput:\n  path: adsl.csv\n  columns: [USUBJID]\n"
        )
        result, texts = converted(
            {"parent.yaml": parent, "spec.yaml": child, "dm.csv": "USUBJID,RACE\n"},
            "spec.yaml",
        )
        text = texts["spec.yaml"]
        self.assertIn(
            "parent: parent.yaml\nclear:\n  - input.DM.require_match\n"
            "  # drops the code\n  - columns.RACEN.derivation.mapping.dict['NOT REPORTED']\n"
            "  - columns.USUBJID.label\n",
            text,
        )
        self.assertNotIn(
            "RACEN", text.split("clear:")[1].split("\n  - columns.USUBJID.label")[1]
        )
        self.assertNotIn("metadata", text)
        self.assertNotIn("input:", text)


class KeyTests(unittest.TestCase):
    """D23, D63: today's inferred keys written out."""

    def test_intermediates_and_joined_inputs_declare_today_s_keys(self):
        spec = (
            "schema_version: '1.0'\ndomain: ADAE\nkeys: [STUDYID, USUBJID, AESEQ]\n"
            "input:\n  AE: ae.csv\n  ADSL: adsl.csv\n"
            "  EX: {path: ex.csv, types: {EXDOSE: float, EXSTDY: int, EXENDY: int, EXSEQ: int}}\n"
            "  DS:\n    path: ds.csv\nbase: AE\n"
            "output:\n  path: adae.csv\n  columns: [STUDYID, USUBJID, AESEQ]\n"
            "intermediates:\n  - id: DEATH\n    dataset: DS\n    filter: \"DS.DSDECOD = 'DEATH'\"\n"
            "  - id: SEL\n    dataset: AE\n    key: [USUBJID]\n"
            "columns:\n  - name: STUDYID\n    type: str\n    derivation: AE.STUDYID\n"
            "  - name: USUBJID\n    type: str\n    derivation: AE.USUBJID\n"
            "  - name: AESEQ\n    type: int\n    derivation: AE.AESEQ\n"
            "  - name: TRTA\n    type: str\n    derivation: ADSL.TRT01A\n"
            '  - name: DOSE\n    type: float\n    derivation:\n      aggregate: "SUM(EX.EXDOSE)"\n'
            "  - name: DTH\n    type: str\n    derivation: DEATH.DSDECOD\n"
        )
        files = {
            "spec.yaml": spec,
            "ae.csv": "STUDYID,USUBJID,AESEQ\n",
            "adsl.csv": "STUDYID,USUBJID,TRT01A\n",
            "ex.csv": "STUDYID,USUBJID,EXDOSE,EXSEQ\n",
            "ds.csv": "STUDYID,USUBJID,DSDECOD\n",
            "benchmark.yaml": "lifecycle: draft\nstandard: ADaM\ndomain: ADAE\n",
        }
        result, texts = converted(files)
        text = texts["spec.yaml"]
        self.assertIn("  ADSL: {path: adsl.csv, key: [STUDYID, USUBJID]}\n", text)
        self.assertIn("EXSEQ: int},\n       key: [STUDYID, USUBJID]}\n", text)
        self.assertIn("  AE: ae.csv\n", text)
        self.assertIn("    dataset: DS\n    key: [STUDYID, USUBJID]\n", text)
        self.assertIn("  DS:\n    path: ds.csv\nbase: AE\n", text)
        self.assertIn("model: ADaM\n", text)
        self.assertEqual(result.stats["input key declarations (P-JOIN-1)"], 2)
        self.assertEqual(result.stats["intermediate keys (D23)"], 1)

    def test_row_template_drivers_stay_keyless_and_a_missing_key_is_listed(self):
        spec = (
            "schema_version: '1.0'\ndomain: VS\nkeys: [USUBJID, VSSEQ]\n"
            "input:\n  VS: vs.csv\n  REF: ref.csv\n"
            "output:\n  path: vs.csv\n  columns: [USUBJID, VSSEQ]\n"
            'intermediates:\n  - id: KEPT\n    dataset: VS\n    filter: "VS.VSSTAT IS NULL"\n'
            '  - id: LIMITS\n    dataset: REF\n    filter: "REF.X = 1"\n'
            "columns:\n  - name: USUBJID\n    type: str\n  - name: VSSEQ\n    type: int\n"
            "  - name: LO\n    type: str\n    derivation: LIMITS.LO\n"
            "rows:\n  - id: kept\n    dataset: KEPT\n    derivations:\n"
            "      USUBJID: KEPT.USUBJID\n      VSSEQ: KEPT.VSSEQ\n"
        )
        files = {
            "spec.yaml": spec,
            "vs.csv": "USUBJID,VSSEQ,VSSTAT\n",
            "ref.csv": "X,LO\n",
        }
        result, texts = converted(files)
        self.assertNotIn("key:", texts["spec.yaml"])
        self.assertEqual(
            result.stats["intermediate row-template drivers left keyless (D23-R1)"], 1
        )
        self.assertIn(
            "intermediate LIMITS: no inferable key",
            "\n".join(note.message for note in result.notes),
        )

    def test_parquet_and_producer_fields_supply_the_keys(self):
        import pyarrow
        import pyarrow.parquet

        producer = (
            "schema_version: '1.0'\ndomain: DM\nkeys: [STUDYID, USUBJID]\ninput:\n  RAW: raw.csv\n"
            "output:\n  path: dm.parquet\n  columns: [STUDYID, USUBJID, RFSTDTC]\n"
            "columns:\n  - {name: STUDYID, type: str}\n  - {name: USUBJID, type: str}\n"
            "  - {name: RFSTDTC, type: date}\n"
        )
        spec = (
            "schema_version: '1.0'\ndomain: ADSL\nkeys: [STUDYID, USUBJID]\n"
            "input:\n  SUBJ: subj.csv\n  DM: {path: dm.parquet, schema: dm.yaml}\n"
            "  PQ: wt.parquet\nbase: SUBJ\n"
            "output:\n  path: adsl.csv\n  columns: [STUDYID, USUBJID]\n"
            "columns:\n  - {name: STUDYID, type: str, derivation: SUBJ.STUDYID}\n"
            "  - {name: USUBJID, type: str, derivation: SUBJ.USUBJID}\n"
            "  - {name: RFSTDTC, type: date, derivation: DM.RFSTDTC}\n"
            "  - {name: WT, type: float, derivation: PQ.WT}\n"
        )
        with tempfile.TemporaryDirectory() as directory:
            case = tree(
                Path(directory) / "case",
                {
                    "spec.yaml": spec,
                    "dm.yaml": producer,
                    "subj.csv": "STUDYID,USUBJID\n",
                    "raw.csv": "A\n",
                },
            )
            pyarrow.parquet.write_table(
                pyarrow.table({"USUBJID": ["1"], "WT": [70.0]}), case / "wt.parquet"
            )
            result = convert([case / "spec.yaml"])
        text = result.files["spec.yaml"]
        self.assertIn(
            "DM: {path: dm.parquet, schema: dm.yaml, key: [STUDYID, USUBJID]}", text
        )
        self.assertIn("PQ: {path: wt.parquet, key: [USUBJID]}", text)

    def test_a_driver_read_on_its_own_rows_is_not_a_join(self):
        spec = (
            "schema_version: '1.0'\ndomain: VS\nkeys: [USUBJID, VSCAT]\n"
            "input:\n  CLINIC: clinic.csv\n  HOME: home.csv\n"
            "output:\n  path: vs.csv\n  columns: [USUBJID, VSCAT, VISIT]\n"
            "columns:\n  - {name: USUBJID, type: str}\n  - {name: VSCAT, type: str}\n"
            "  - {name: VISIT, type: str, derivation: CLINIC.VISIT}\n"
            "rows:\n  - id: clinic\n    dataset: CLINIC\n    derivations: {USUBJID: CLINIC.USUBJID, VSCAT: {literal: C}}\n"
            "  - id: home\n    dataset: HOME\n    group_by: [HOME.USUBJID]\n"
            "    derivations:\n      USUBJID: HOME.USUBJID\n      VSCAT: {literal: H}\n"
            "      {visit}\n"
        )
        files = {"clinic.csv": "USUBJID,VSCAT,VISIT\n", "home.csv": "USUBJID,SYSBP\n"}
        _, texts = converted(
            {**files, "spec.yaml": spec.replace("{visit}", "VISIT: {literal: null}")}
        )
        self.assertNotIn("key:", texts["spec.yaml"])
        result, texts = converted(
            {**files, "spec.yaml": spec.replace("      {visit}\n", "")}
        )
        self.assertIn(
            "CLINIC: {path: clinic.csv, key: [USUBJID, VSCAT]}", texts["spec.yaml"]
        )
        self.assertEqual(
            result.stats["join pairs on rows another template's driver built (NAME-4)"],
            1,
        )

    def test_a_grouped_row_aggregate_over_its_driver_needs_no_key(self):
        spec = (
            "schema_version: '1.0'\ndomain: VS\nkeys: [USUBJID]\ninput:\n  HOME: home.csv\n"
            "output:\n  path: vs.csv\n  columns: [USUBJID, MEAN]\n"
            "columns:\n  - {name: USUBJID, type: str}\n  - {name: MEAN, type: float}\n"
            "rows:\n  - id: day\n    dataset: HOME\n    group_by: [HOME.USUBJID]\n"
            '    derivations:\n      USUBJID: HOME.USUBJID\n      MEAN: {aggregate: "MEAN(HOME.SYSBP)"}\n'
        )
        _, texts = converted({"spec.yaml": spec, "home.csv": "USUBJID,SYSBP\n"})
        self.assertNotIn("key:", texts["spec.yaml"])

    def test_a_layer_declaring_the_input_takes_its_key(self):
        parent = (
            "schema_version: '1.0'\ninput:\n  LB: lb.csv\n  DM: dm.csv\nbase: LB\n"
            "columns:\n  - {name: USUBJID, type: str, derivation: LB.USUBJID}\n"
            "  - {name: AGE, type: int, derivation: DM.AGE}\n"
        )
        child = (
            "schema_version: '1.0'\nparent: parent.yaml\ndomain: ADLB\nkeys: [USUBJID]\n"
            "output:\n  path: adlb.csv\n  columns: [USUBJID, AGE]\n"
        )
        files = {
            "parent.yaml": parent,
            "spec_study.yaml": child,
            "lb.csv": "USUBJID\n",
            "dm.csv": "USUBJID,AGE\n",
            "benchmark.yaml": "lifecycle: draft\nstandard: ADaM\ndomain: ADLB\nentries:\n"
            "  python:\n  - specification: spec_study.yaml\n  r:\n  - specification: spec_study.yaml\n",
        }
        _, texts = converted(files)
        self.assertIn("  DM: {path: dm.csv, key: [USUBJID]}\n", texts["parent.yaml"])
        self.assertNotIn("model", texts["parent.yaml"])
        self.assertIn("model: ADaM\n", texts["spec_study.yaml"])


class ProjectTests(unittest.TestCase):
    """D8: environment files merge into function files; P-MODEL-1 from sections."""

    ENVIRONMENT = (
        "schema_version: '1.0'\n{lock}functions:\n  f:\n    function: {callable}\n"
        "    description: A function.\n    params:\n    - name: x\n      type: date\n"
        "      required: false\n      default: {{date: 2024-01-01}}\n    returns: int\n"
        "    tests:\n    - id: one\n      covers: [normal]\n      args: {{x: 2024-01-02}}\n      result: 1\n"
    )

    def files(self, python_lock="", r_lock="lock: renv.lock\n", r_callable="pkg::f"):
        return {
            "project.yaml": (
                "# the study\nschema_version: '1.0'\nenvironments:\n"
                "  python: python/environment.yaml\n  r: r/environment.yaml\n"
                "adam:\n  specs:\n  - spec.yaml\n"
            ),
            "python/environment.yaml": self.ENVIRONMENT.format(
                lock=python_lock, callable="pkg.f"
            ),
            "r/environment.yaml": self.ENVIRONMENT.format(
                lock=r_lock, callable=r_callable
            ),
            "spec.yaml": "schema_version: '1.0'\ndomain: ADSL\nkeys: [A]\ninput:\n  X: x.csv\n",
            "x.csv": "A\n",
            "benchmark.yaml": "lifecycle: draft\nstandard: TEST\ndomain: ADSL\n",
        }

    def test_the_merge_writes_one_function_file_and_moves_the_locks(self):
        result, texts = converted(self.files(python_lock="lock: ../uv.lock\n"))
        self.assertEqual(
            texts["functions/f.yaml"],
            'yamaa: "1.0"\ntype: function\nname: f\ndescription: A function.\n'
            "params:\n- name: x\n  type: date\n  required: false\n  default: 2024-01-01\n"
            "returns: int\nimplementations:\n  python: pkg.f\n  r: pkg::f\n"
            "tests:\n- id: one\n  covers: [normal]\n  args: {x: 2024-01-02}\n  result: 1\n",
        )
        self.assertEqual(
            texts["project.yaml"],
            '# the study\nyamaa: "1.0"\ntype: project\nfunctions:\n- functions/f.yaml\n'
            "environments:\n  python:\n    lock: uv.lock\n  r:\n    lock: r/renv.lock\n"
            "adam:\n  specs:\n  - spec.yaml\n",
        )
        self.assertEqual(
            sorted(result.removed),
            ["case/python/environment.yaml", "case/r/environment.yaml"],
        )
        self.assertIn("model: ADaM\n", texts["spec.yaml"])

    def test_no_lock_leaves_no_environments_and_a_difference_is_listed(self):
        result, texts = converted(self.files(r_lock="", r_callable="pkg::g"))
        self.assertNotIn("environments", texts["project.yaml"])
        self.assertIn("  r: pkg::g\n", texts["functions/f.yaml"])
        files = self.files()
        files["r/environment.yaml"] = files["r/environment.yaml"].replace(
            "returns: int", "returns: float"
        )
        result, _ = converted(files)
        self.assertIn(
            "differ beyond `function`", "\n".join(note.message for note in result.notes)
        )


class CodelistTests(unittest.TestCase):
    def test_a_codelist_file_a_project_names_gains_its_header(self):
        files = {
            "project.yaml": "schema_version: '1.0'\ncodelists:\n- terms/sponsor.yaml\n"
            "sdtm:\n  specs:\n  - spec.yaml\n",
            "terms/sponsor.yaml": "# sponsor terms\ncodelists:\n- id: YN\n  name: Yes or no\n"
            "  items:\n  - value: Y\n",
            "spec.yaml": "schema_version: '1.0'\ndomain: DM\nkeys: [A]\ninput:\n  X: x.csv\n",
            "x.csv": "A\n",
        }
        result, texts = converted(files)
        self.assertEqual(
            texts["terms/sponsor.yaml"],
            '# sponsor terms\nyamaa: "1.0"\ntype: codelists\ncodelists:\n- id: YN\n'
            "  name: Yes or no\n  items:\n  - value: Y\n",
        )
        self.assertIn("model: SDTM\n", texts["spec.yaml"])


class ContractTests(unittest.TestCase):
    """The paths and conditions blocks applied to negative contracts."""

    def setUp(self):
        from tools.migrate.contracts import rewrite_path

        self.table = table_module.load()
        self.rewrite = rewrite_path
        self.spec = {
            "intermediates": [{"id": "REF"}, {"id": "WIN"}],
            "rows": [{"id": "home"}],
            "columns": [
                {
                    "name": "A",
                    "derivation": {
                        "aggregate": {"derive": [{"name": "D"}, {"name": "E"}]}
                    },
                }
            ],
        }

    def test_each_path_rule(self):
        cases = {
            "intermediates[1].between.value": "intermediates.WIN.between.value",
            "intermediates[0].no_match": "intermediates.REF.absent",
            "rows[0].derivations.X.source": "rows.home.derivations.X.source",
            "columns.A.derivation.compute.expr": "columns.A.derivation",
            "columns.A.derivation.compute": "columns.A.derivation",
            "columns.A.derivation.case[0].then.literal": "columns.A.derivation.case[0].then",
            "columns.A.derivation.study_day.date": "columns.A.derivation.study_day.source",
            "columns.A.derivation.baseline_flag.value": "columns.A.derivation.baseline_flag.source",
            "columns.A.derivation.baseline_flag.reference_date": "columns.A.derivation.baseline_flag.reference",
            "columns.A.derivation.diff.bounds": "columns.A.derivation.diff.closed",
            "columns.A.derivation.aggregate.derive[1].derivation": "columns.A.derivation.aggregate.derivations.E.derivation",
            "sdtm.datasets": "sdtm.transport",
            "adam.standard.version": "adam.standards[0].version",
            "columns.A.submission.origin": "columns.A.submission.origins[0]",
            "columns.A.submission.value_level[0].origin.type": "columns.A.submission.value_level[0].origins[0].type",
            "datasets.DM.columns.INVNAM": "datasets.DM.columns.INVNAM",
            "verifications[0].row_count": "verifications[0].row_count",
        }
        for before, after in cases.items():
            with self.subTest(path=before):
                self.assertEqual(self.rewrite(before, self.spec, [], self.table), after)
        self.assertEqual(
            self.rewrite("columns.A.type", {}, ["columns.A.type"], self.table),
            "clear[0]",
        )
        self.assertIsNone(
            self.rewrite("intermediates[5].key", self.spec, [], self.table)
        )

    def test_a_renamed_condition_follows_the_table_and_a_decided_one_is_listed(self):
        from tools.migrate.contracts import condition_change

        family = self.table.condition("unknown_standard_family")
        self.assertEqual(
            condition_change(family, "any"), ("rename", "standard_model_mismatch")
        )
        mismatch = self.table.condition("schema_version_mismatch")
        self.assertEqual(
            condition_change(mismatch, "negative-version-mismatch")[0], "manual"
        )


class MarkdownTests(unittest.TestCase):
    def test_blocks_convert_in_place_and_prose_is_listed(self):
        readme = (
            "# Case\n\nThe home template writes `X: {literal: null}`.\n\n"
            "1. Fix:\n\n   ```yaml\n   - name: A\n     derivation: {literal: Y}\n   ```\n\n"
            "```yaml\nlifecycle: draft\nstandard: SDTM\ndomain: DM\n```\n\n"
            "```yaml\nthen: null\n```\n"
        )
        result, texts = converted({"README.md": readme}, "README.md")
        text = texts["README.md"]
        self.assertIn("   - name: A\n     derivation: \"'Y'\"\n", text)
        self.assertIn("lifecycle: draft\nstandard: SDTM\n", text)
        self.assertIn("```yaml\nthen: null\n```", text)
        self.assertIn("prose", {note.kind for note in result.notes})
        again, texts_again = converted({"README.md": text}, "README.md")
        self.assertEqual(texts_again["README.md"], text)

    def test_fragment_intermediates_take_keys_from_the_benchmark(self):
        spec = (
            "schema_version: '1.0'\ndomain: ADSL\nkeys: [STUDYID, USUBJID]\n"
            "input:\n  DM: dm.csv\n  EX: ex.csv\nbase: DM\n"
            "output:\n  path: adsl.csv\n  columns: [STUDYID, USUBJID]\n"
            "columns:\n  - {name: STUDYID, type: str, derivation: DM.STUDYID}\n"
            "  - {name: USUBJID, type: str, derivation: DM.USUBJID}\n"
        )
        readme = (
            "# Case\n\n```yaml\nintermediates:\n  - id: FIRST\n    dataset: EX\n"
            "    order_by: [EX.EXSTDTC]\n    keep: first\n    no_match: null\n```\n"
        )
        files = {
            "spec.yaml": spec,
            "README.md": readme,
            "dm.csv": "STUDYID,USUBJID\n",
            "ex.csv": "STUDYID,USUBJID,EXSTDTC\n",
        }
        _, texts = converted(files)
        self.assertIn(
            "  - id: FIRST\n    dataset: EX\n    key: [STUDYID, USUBJID]\n"
            "    order_by: [EX.EXSTDTC]\n    keep: first\n```",
            texts["README.md"],
        )


class SafetyTests(unittest.TestCase):
    def test_the_target_may_not_overlap_a_source(self):
        with tempfile.TemporaryDirectory() as directory:
            case = tree(
                Path(directory) / "case",
                {"spec.yaml": "schema_version: '1.0'\ndomain: DM\n"},
            )
            result = convert([case])
            with self.assertRaises(SourceError):
                write(result, case / "out", [case])
            with self.assertRaises(SourceError):
                write(result, case, [case])

    def test_the_table_and_the_implementation_must_agree(self):
        table = table_module.load()
        with self.assertRaises(table_module.TableError):
            table.check_implemented(
                {**{name: None for name in table.transforms}, "extra": None}, {}
            )

    def test_an_unapproved_table_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "renames.yaml"
            text = table_module.TABLE.read_text(encoding="utf-8")
            path.write_text(
                text.replace("status: approved", "status: proposed", 1),
                encoding="utf-8",
            )
            with self.assertRaises(table_module.TableError):
                table_module.load(path)

    def test_the_repository_yaml_directory_cannot_stand_in_for_pyyaml(self):
        # Without site-packages there is no PyYAML, so `import yaml` from the
        # repository root finds its yaml/ directory, as in an environment that
        # was never synced. The converter names what it found instead of
        # failing later on a missing loader.
        result = subprocess.run(
            [
                sys.executable,
                "-I",
                "-S",
                "-c",
                "import sys; sys.path.insert(0, sys.argv[1]); import tools.migrate",
                str(ROOT),
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(
            "ImportError: tools.migrate needs PyYAML, but `import yaml` found "
            f"{ROOT / 'yaml'}, which is not PyYAML",
            result.stderr,
        )
        self.assertNotIn("AttributeError", result.stderr)


if __name__ == "__main__":
    unittest.main()
