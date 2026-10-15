import ast
import copy
import decimal as OWNER_DECIMAL
import importlib
import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import yaml
from tools.paths import ROOT
from tools.staging import stage_grammar_inputs
from tools.validation import _native_odm as OWNER_NATIVE_ODM
from tools.validation import _native_specification as OWNER_NATIVE_SPECIFICATION
from tools.validation import _repository_yaml as OWNER_REPOSITORY_YAML
from tools.validation import _semantic_controls as CONTROLS
from tools.validation import _source_controls as SOURCE_CONTROLS
from tools.validation import _typed_fixtures as OWNER_TYPED_FIXTURES
from tools.validation import check_execution_manifest as EXECUTION_CHECK
from tools.validation import check_rule_metadata as RULE_CHECK
from tools.validation import check_validation_blockers as BLOCKER_CHECK
from tools.validation import editorial as OWNER_EDITORIAL
from tools.validation import grammar_checks as OWNER_GRAMMAR_CHECKS
from tools.validation import regex_checks as OWNER_REGEX_CHECKS
from tools.validation import repository_artifacts as OWNER_REPOSITORY_ARTIFACTS
from tools.validation import repository_manifests as OWNER_REPOSITORY_MANIFESTS
from tools.validation import repository_schema as OWNER_REPOSITORY_SCHEMA
from tools.validation import repository_structure as OWNER_REPOSITORY_STRUCTURE
from tools.validation import validate_repository as VALIDATOR
from tools.validation import validation_diagnostics as OWNER_VALIDATION_DIAGNOSTICS

TOOL_PATH = Path(VALIDATOR.__file__)


BLOCKER_PATH = Path(BLOCKER_CHECK.__file__)


EXECUTION_PATH = Path(EXECUTION_CHECK.__file__)


def typed_schema_control(specification, label, _path, environment, *_args, **_kwargs):
    """Isolate original custom-schema truth from whole-spec version admission."""
    schema = {"version": environment.get("version", "1.0"), **environment["classes"]}
    observation = OWNER_TYPED_FIXTURES.observe(
        "schema", json.dumps([yaml.safe_dump(schema), json.dumps(specification)])
    )
    return [
        OWNER_VALIDATION_DIAGNOSTICS.validation_diagnostic(
            label + ("." + path if path else ""),
            row["condition"],
            row["condition"] + ": " + json.dumps(row["context"]),
            context=row["context"],
        )
        for row in observation["issues"]
        for path in row["spec_paths"]
    ]


RULE_PATH = Path(RULE_CHECK.__file__)


class TestSemanticFinding(unittest.TestCase):
    def test_native_findings_keep_structured_message_and_own_their_context(self):
        row = {
            "phase": "validation",
            "condition": "unknown_field",
            "requirement": "REQ-0103",
            "spec_paths": ["columns.X"],
            "context": {"identifier": "SRC.UNKNOWN"},
            "span": [1, 12],
        }
        finding = CONTROLS.Finding(row, "spec")
        self.assertIsInstance(
            finding, OWNER_VALIDATION_DIAGNOSTICS.ValidationDiagnostic
        )
        self.assertEqual(
            str(finding),
            'ERROR: spec.columns.X: unknown_field: {"identifier": "SRC.UNKNOWN"}',
        )
        self.assertEqual(
            finding.message, 'unknown_field: {"identifier": "SRC.UNKNOWN"}'
        )
        self.assertEqual(finding.path, "spec.columns.X")
        self.assertEqual(finding.span, (1, 12))
        self.assertEqual(finding.requirement, "REQ-0103")
        self.assertEqual(finding.phase, "validation")
        row["context"]["identifier"] = "changed"
        self.assertEqual(finding.context, {"identifier": "SRC.UNKNOWN"})


class TestYamlLoader(unittest.TestCase):
    def test_yaml_12_boolean_resolution(self):
        loaded = yaml.load(
            "yes_value: yes\n"
            "on_value: ON\n"
            "true_value: true\n"
            "title_true_value: True\n"
            "upper_false_value: FALSE\n"
            "mixed_true_value: TrUe\n"
            "mixed_false_value: FaLsE\n",
            Loader=OWNER_REPOSITORY_YAML.UniqueKeyLoader,
        )
        self.assertEqual(loaded["yes_value"], "yes")
        self.assertEqual(loaded["on_value"], "ON")
        self.assertIs(loaded["true_value"], True)
        self.assertIs(loaded["title_true_value"], True)
        self.assertIs(loaded["upper_false_value"], False)
        self.assertEqual(loaded["mixed_true_value"], "TrUe")
        self.assertEqual(loaded["mixed_false_value"], "FaLsE")

    def test_yaml_12_core_scalar_resolution(self):
        loaded = yaml.load(
            "date_value: 2025-01-02\n"
            "decimal_value: 012\n"
            "octal_value: 0o12\n"
            "signed_octal_value: -0o12\n"
            "sexagesimal_value: 1:20\n"
            "exponent_value: 1e3\n"
            "underscored_value: 1_000\n",
            Loader=OWNER_REPOSITORY_YAML.UniqueKeyLoader,
        )
        self.assertEqual(loaded["date_value"], "2025-01-02")
        self.assertEqual(loaded["decimal_value"], 12)
        self.assertEqual(loaded["octal_value"], 10)
        self.assertEqual(loaded["signed_octal_value"], "-0o12")
        self.assertEqual(loaded["sexagesimal_value"], "1:20")
        self.assertEqual(loaded["exponent_value"], 1000.0)
        self.assertEqual(loaded["underscored_value"], "1_000")

    def test_non_finite_yaml_floats_normalize_to_missing(self):
        spellings = [
            ".inf",
            ".Inf",
            ".INF",
            "+.inf",
            "+.Inf",
            "+.INF",
            "-.inf",
            "-.Inf",
            "-.INF",
            ".nan",
            ".NaN",
            ".NAN",
        ]
        loaded = yaml.load(
            "".join(
                f"value_{index}: {spelling}\n"
                for index, spelling in enumerate(spellings)
            )
            + "quoted: '.inf'\n",
            Loader=OWNER_REPOSITORY_YAML.UniqueKeyLoader,
        )

        for index, spelling in enumerate(spellings):
            with self.subTest(spelling=spelling):
                self.assertIsNone(loaded[f"value_{index}"])
        self.assertEqual(loaded["quoted"], ".inf")

    def test_aliases_are_rejected(self):
        with self.assertRaises(yaml.YAMLError):
            yaml.load(
                "first: &value text\nsecond: *value\n",
                Loader=OWNER_REPOSITORY_YAML.UniqueKeyLoader,
            )

    def test_merge_keys_are_rejected(self):
        with self.assertRaises(yaml.YAMLError):
            yaml.load(
                "base: &base {a: 1}\nmerged: {<<: *base}\n",
                Loader=OWNER_REPOSITORY_YAML.UniqueKeyLoader,
            )

    def test_merge_keys_report_the_intended_reason(self):
        # The merge-key guard lives in flatten_mapping, which only runs when
        # construct_mapping calls it. This anchor-free merge key is the only
        # shape that reaches the guard (anchors/aliases die at the composer),
        # so it pins the intended message instead of the accidental
        # "could not determine a constructor for the tag
        # 'tag:yaml.org,2002:merge'".
        with self.assertRaisesRegex(yaml.YAMLError, "YAML merge keys are not allowed"):
            yaml.load(
                "merged:\n  <<: just_a_scalar\n",
                Loader=OWNER_REPOSITORY_YAML.UniqueKeyLoader,
            )

    def test_explicit_tags_are_rejected(self):
        with self.assertRaises(yaml.YAMLError):
            yaml.load(
                "value: !!str 123\n",
                Loader=OWNER_REPOSITORY_YAML.UniqueKeyLoader,
            )


class TestTextSourceBoundary(unittest.TestCase):
    def test_preserves_only_named_third_party_unicode_data_and_notices(self):
        """Exact notice/data paths are exempt; neighboring project text stays checked."""
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            for relative in (
                "rust/crates/yamaa-core/unicode/18.0.0/DerivedCoreProperties.txt",
                "rust/crates/yamaa-core/unicode/LICENSE.txt",
                "rust/crates/yamaa-python/LICENSE-UNICODE.txt",
                "R/yamaa/inst/LICENSE-UNICODE.txt",
                "python/LICENSE-UNICODE.txt",
            ):
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(chr(0x00A9), encoding="utf-8")
            for relative in (
                "LICENSE-UNICODE.txt",
                "rust/crates/yamaa-core/unicode/notes.md",
            ):
                (root / relative).write_text(chr(0x00A9), encoding="utf-8")
            errors = OWNER_EDITORIAL.validate_ascii_sources(root)
        self.assertEqual(
            errors,
            [
                "ERROR: LICENSE-UNICODE.txt:1:1: non_ascii_source byte 0xC2",
                "ERROR: rust/crates/yamaa-core/unicode/notes.md:1:1: non_ascii_source byte 0xC2",
            ],
        )

    def test_rejects_non_ascii_source_but_allows_unicode_csv_data(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            source = root / "rule.md"
            source.write_bytes(b"# Rule\ntext " + bytes([0xC3, 0xA9]) + b"\n")
            for fixture_type in ("input", "expected"):
                data = root / "benchmarks" / "example" / fixture_type
                data.mkdir(parents=True)
                (data / "values.csv").write_text(
                    "VALUE\n" + chr(0x00E9) + "\n",
                    encoding="utf-8",
                )

            errors = OWNER_EDITORIAL.validate_ascii_sources(root)
            csv_errors = OWNER_REPOSITORY_ARTIFACTS.validate_csv_shapes(root)

        self.assertEqual(
            errors,
            ["ERROR: rule.md:2:6: non_ascii_source byte 0xC3"],
        )
        self.assertEqual(csv_errors, [])

    def test_rejects_non_ascii_csv_outside_fixture_directories(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            (root / "reference.csv").write_text(
                "VALUE\n" + chr(0x00E9) + "\n",
                encoding="utf-8",
            )

            errors = OWNER_EDITORIAL.validate_ascii_sources(root)

        self.assertEqual(
            errors,
            ["ERROR: reference.csv:2:1: non_ascii_source byte 0xC3"],
        )

    def test_preserves_rust_json_fixture_data_and_checks_neighboring_source(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            for relative in (
                "rust/crates/yamaa-core/tests/fixtures/cases.json",
                "rust/crates/yamaa-adapters/tests/fixtures/nested/cases.json",
                "rust/crates/yamaa-core/tests/fixtures/notes.md",
                "rust/crates/yamaa-core/tests/fixtures/helper.py",
                "rust/crates/yamaa-core/tests/metadata.json",
                "cases.json",
            ):
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(chr(0x00E9), encoding="utf-8")

            errors = OWNER_EDITORIAL.validate_ascii_sources(root)

        self.assertEqual(
            errors,
            [
                "ERROR: cases.json:1:1: non_ascii_source byte 0xC3",
                "ERROR: rust/crates/yamaa-core/tests/fixtures/helper.py:1:1: non_ascii_source byte 0xC3",
                "ERROR: rust/crates/yamaa-core/tests/fixtures/notes.md:1:1: non_ascii_source byte 0xC3",
                "ERROR: rust/crates/yamaa-core/tests/metadata.json:1:1: non_ascii_source byte 0xC3",
            ],
        )

    def test_rejects_a_decoded_surrogate(self):
        document = {"value": chr(0xD800)}

        errors = OWNER_EDITORIAL.validate_unicode_scalars(document, "spec.yaml")

        self.assertEqual(
            errors,
            [
                "ERROR: spec.yaml.value: invalid_text surrogate U+D800 "
                "at string offset 0"
            ],
        )

    def test_escapes_a_surrogate_mapping_key_in_value_diagnostics(self):
        key = chr(0xD800)
        document = {key: chr(0xD801)}

        errors = OWNER_EDITORIAL.validate_unicode_scalars(document, "spec.yaml")

        self.assertEqual(
            errors,
            [
                "ERROR: spec.yaml.<key>: invalid_text surrogate U+D800 "
                "at string offset 0",
                "ERROR: spec.yaml.\\ud800: invalid_text surrogate U+D801 "
                "at string offset 0",
            ],
        )
        for error in errors:
            error.encode("ascii")

    def test_rejects_ill_formed_utf8_data(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            data = Path(temp_dir) / "benchmarks" / "example" / "input"
            data.mkdir(parents=True)
            (data / "values.csv").write_bytes(b"VALUE\n" + bytes([0xFF]) + b"\n")

            errors = OWNER_REPOSITORY_ARTIFACTS.validate_csv_shapes(Path(temp_dir))

        self.assertEqual(len(errors), 1)
        self.assertIn("invalid_text", errors[0])


class TestPredicateLanguage(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        CONTROLS.syntax("predicate", "TRUE")

    def test_parses_precedence_compounds_and_all_literal_types(self):
        predicate = (
            "NOT FLAG = 'N' OR "
            "COUNT BETWEEN -1 AND +2.5e1 AND "
            "TERM LIKE '100!!%' ESCAPE '!' AND "
            "DAY >= DATE '2025-01-01' AND "
            "MOMENT < DATETIME '2025-01-02T03:04' AND "
            "OPTION IN ('A', 'B', NULL)"
        )

        ast = CONTROLS.predicate_syntax(predicate)
        types = {
            "FLAG": "str",
            "COUNT": "int",
            "TERM": "str",
            "DAY": "date",
            "MOMENT": "datetime",
            "OPTION": "str",
        }
        errors = CONTROLS.predicate_ast(ast, types.get)

        self.assertEqual(ast["kind"], "or")
        self.assertEqual(errors, [])

    def test_rejects_syntax_outside_the_closed_grammar(self):
        invalid = [
            "VALUE != 1",
            "VALUE + 1 > 2",
            "VALUE",
            "VALUE IN ()",
            "VALUE = 'unterminated",
            "DAY = DATE '2025-02-30'",
            "TERM LIKE 'abc!' ESCAPE '!'",
        ]

        for predicate in invalid:
            with self.subTest(predicate=predicate):
                with self.assertRaises(CONTROLS.SyntaxError):
                    CONTROLS.predicate_syntax(predicate)

    def test_allows_an_escaped_escape_at_the_end_of_like_pattern(self):
        ast = CONTROLS.predicate_syntax("TERM LIKE 'abc!!' ESCAPE '!'")
        errors = CONTROLS.predicate_ast(ast, {"TERM": "str"}.get)
        self.assertEqual(errors, [])

    def test_rejects_unknown_and_incompatible_operands(self):
        ast = CONTROLS.predicate_syntax(
            "UNKNOWN = 1 OR DAY = '2025-01-01' OR COUNT LIKE '1%'"
        )

        errors = CONTROLS.predicate_ast(ast, {"DAY": "date", "COUNT": "int"}.get)

        self.assertEqual(
            [error.condition for error in errors],
            ["unknown_field", "incompatible_input_type", "incompatible_input_type"],
        )
        self.assertEqual(errors[0].context["identifier"], "UNKNOWN")
        self.assertIn("date", errors[1].context.values())
        self.assertIn("str", errors[1].context.values())
        self.assertIn("int", errors[2].context.values())
        self.assertIn("str", errors[2].context.values())

    def test_validates_predicate_sites_in_a_specification(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            example_dir = Path(temp_dir)
            source = example_dir / "dm.csv"
            source.write_text("USUBJID,AGE\n01,40\n")
            spec_path = example_dir / "spec.yaml"
            spec = {
                "domain": "ADSL",
                "input": {
                    "DM": {
                        "path": "dm.csv",
                        "types": {"AGE": "int"},
                    }
                },
                "base": "DM",
                "keys": ["USUBJID"],
                "output": {"path": "out.csv", "columns": ["USUBJID", "AGE"]},
                "columns": [
                    {
                        "name": "USUBJID",
                        "type": "str",
                        "derivation": {"source": "DM.USUBJID"},
                    },
                    {
                        "name": "AGE",
                        "type": "int",
                        "derivation": {
                            "case": [
                                {
                                    "when": "DM.AGE >= 18",
                                    "then": {"source": "DM.AGE"},
                                }
                            ]
                        },
                    },
                ],
                "verifications": [
                    {
                        "assert": {
                            "id": "known-age",
                            "when": "GUARD = 1",
                            "require": "MISSING > 0",
                        }
                    }
                ],
            }

            prefix = CONTROLS.document(spec, "example/spec.yaml", spec_path)
            self.assertEqual(
                [(error.path, error.condition, error.context) for error in prefix],
                [
                    (
                        "example/spec.yaml.verifications[0].assert.when",
                        "unknown_field",
                        {"identifier": "GUARD"},
                    )
                ],
            )
            # The installed verifier keeps its first deferred failure. A second
            # authored control makes the require clause independently reachable.
            independent = copy.deepcopy(spec)
            independent["verifications"][0]["assert"]["when"] = "TRUE"
            errors = prefix + CONTROLS.document(
                independent, "example/spec.yaml", spec_path
            )

        # REQ-0383: an assert's `when` is a predicate site of its own, so
        # each field reports at its own path.
        self.assertEqual(len(errors), 2)
        self.assertIn("verifications[0].assert.when", errors[0])
        self.assertEqual(errors[0].context, {"identifier": "GUARD"})
        self.assertIn("verifications[0].assert.require", errors[1])
        self.assertEqual(errors[1].context, {"identifier": "MISSING"})

    def test_validates_the_predicate_a_source_filter_declares(self):
        # REQ-0132: the filter selects right-side records, so it resolves
        # against the dataset the source reads and not against the output.
        spec = {
            "domain": "DM",
            "input": {"ODM": "odm.csv"},
            "base": "ODM",
            "keys": ["USUBJID"],
            "output": {"path": "out.csv", "columns": ["USUBJID", "SEX", "AGE", "ARM"]},
            "columns": [
                {
                    "name": "USUBJID",
                    "type": "str",
                    "derivation": {"source": "ODM.SubjectKey"},
                },
                {
                    "name": "SEX",
                    "type": "str",
                    "derivation": {
                        "mapping": {
                            "source": {
                                "variable": "ODM.Value",
                                "filter": "ODM.Unknown = 1",
                            },
                            "dict": {"Male": "M"},
                        }
                    },
                },
                {
                    "name": "AGE",
                    "type": "int",
                    "derivation": {
                        "source": {
                            "variable": "ODM.Value",
                            "filter": "USUBJID = '01'",
                        }
                    },
                },
                {
                    "name": "ARM",
                    "type": "str",
                    "derivation": {
                        "first_available": {
                            "sources": [
                                {
                                    "variable": "ODM.Value",
                                    "filter": "ODM.Missing IS NULL",
                                }
                            ],
                            "missing": "Unassigned",
                        }
                    },
                },
            ],
        }

        with tempfile.TemporaryDirectory() as temp_dir:
            example_dir = Path(temp_dir)
            (example_dir / "odm.csv").write_text(
                "SubjectKey,ItemOID,Value\n01,IT.DM.SEX,Male\n"
            )
            spec_path = example_dir / "spec.yaml"
            errors = CONTROLS.document(spec, "example/spec.yaml", spec_path)
        message = "\n".join(errors)

        self.assertEqual(len(errors), 3)
        self.assertIn("columns.SEX.derivation.mapping.source.filter", message)
        self.assertIn("columns.AGE.derivation.source.filter", message)
        self.assertIn(
            "columns.ARM.derivation.first_available.sources[0].filter", message
        )
        self.assertEqual(
            [error.context["identifier"] for error in errors],
            ["ODM.Unknown", "USUBJID", "ODM.Missing"],
        )
        self.assertEqual([error.condition for error in errors], ["unknown_field"] * 3)

    def test_an_ungrouped_row_filter_reads_derived_columns_and_lookups(self):
        # REQ-0068: the filter evaluates after the record's derivations, so
        # it reads the driver, the template's derived columns, and lookup
        # state, but not a column another template derives.
        spec = {
            "domain": "ADQS",
            "input": {
                "ADSL": "adsl.csv",
                "QS": {"path": "qs.csv", "types": {"AVISITN": "int", "AVAL": "float"}},
            },
            "keys": ["USUBJID", "AVISITN"],
            "intermediates": [
                {"id": "COLLECTED", "dataset": "QS", "key": ["USUBJID", "AVISITN"]}
            ],
            "output": {"path": "out.csv", "columns": ["USUBJID", "AVISITN"]},
            "columns": [
                {"name": "USUBJID", "type": "str"},
                {"name": "AVISITN", "type": "int"},
                {"name": "OTHER", "type": "str"},
            ],
            "rows": [
                {
                    "id": "missed",
                    "dataset": "ADSL",
                    "filter": (
                        "ADSL.EFFFL = 'Y' AND AVISITN = 8 AND COLLECTED.AVAL IS NULL"
                    ),
                    "derivations": {
                        "USUBJID": "ADSL.USUBJID",
                        "AVISITN": {"literal": 8},
                        "OTHER": {"literal": "A"},
                    },
                },
                {
                    "id": "unknown",
                    "dataset": "QS",
                    "filter": "OTHER = 'A' OR COLLECTED.NOSUCH IS NULL",
                    "derivations": {
                        "USUBJID": "QS.USUBJID",
                        "AVISITN": "QS.AVISITN",
                    },
                },
            ],
        }

        with tempfile.TemporaryDirectory() as temp_dir:
            example_dir = Path(temp_dir)
            (example_dir / "adsl.csv").write_text("USUBJID,EFFFL\n01,Y\n")
            (example_dir / "qs.csv").write_text("USUBJID,AVISITN,AVAL\n01,8,3\n")
            incomplete = CONTROLS.document(
                spec, "example/spec.yaml", example_dir / "spec.yaml"
            )
            self.assertEqual(
                [(error.condition, error.context) for error in incomplete],
                [("missing_derivation", {"column": "OTHER", "rows": ["unknown"]})],
            )
            spec["columns"][2]["derivation"] = {"literal": None}
            errors = CONTROLS.document(
                spec, "example/spec.yaml", example_dir / "spec.yaml"
            )
        message = "\n".join(errors)

        self.assertEqual(len(errors), 1, message)
        self.assertNotIn("rows[0].filter", message)
        self.assertIn("rows[1].filter", message)
        self.assertEqual(errors[0].context, {"identifier": "COLLECTED.NOSUCH"})
        self.assertEqual(errors[0].condition, "unknown_field")

    def test_an_ungrouped_row_filter_rejects_a_window_derived_column(self):
        # REQ-0068: the filter evaluates before the window pass, so naming
        # a window-derived column fails -- directly or through a derivation
        # that reads one -- while a grouped filter may name one. The
        # driver carries AVISITN raw, typed as in QS: row-context joins
        # match raw driver keys (f4308beb), so a driver without the key
        # would be refused on that ground before the phase check.
        def window_derivation():
            return {
                "locf": {
                    "source": "RAW_AVAL",
                    "window": {
                        "group_by": ["USUBJID"],
                        "order_by": ["USUBJID"],
                    },
                }
            }

        spec = {
            "domain": "ADQS",
            "input": {
                "ADSL": {"path": "adsl.csv", "types": {"AVISITN": "int"}},
                "QS": {"path": "qs.csv", "types": {"AVISITN": "int", "AVAL": "float"}},
            },
            "keys": ["USUBJID", "AVISITN"],
            "output": {
                "path": "out.csv",
                "columns": ["USUBJID", "AVISITN", "AVAL", "DOUBLED"],
            },
            "columns": [
                {"name": "USUBJID", "type": "str"},
                {"name": "AVISITN", "type": "int"},
                {"name": "RAW_AVAL", "type": "float", "derivation": "QS.AVAL"},
                {"name": "AVAL", "type": "float"},
                {"name": "DOUBLED", "type": "float", "derivation": {"literal": None}},
            ],
            "rows": [
                {
                    "id": "direct",
                    "dataset": "ADSL",
                    "filter": "AVAL > 20",
                    "derivations": {
                        "USUBJID": "ADSL.USUBJID",
                        "AVISITN": {"literal": 8},
                        "AVAL": window_derivation(),
                    },
                },
                {
                    "id": "transitive",
                    "dataset": "ADSL",
                    "filter": "DOUBLED > 40",
                    "derivations": {
                        "USUBJID": "ADSL.USUBJID",
                        "AVISITN": {"literal": 16},
                        "AVAL": window_derivation(),
                        "DOUBLED": "AVAL",
                    },
                },
                {
                    "id": "grouped",
                    "dataset": "ADSL",
                    "group_by": ["ADSL.USUBJID", "ADSL.AVISITN"],
                    "filter": "AVAL > 20",
                    "derivations": {
                        "USUBJID": "ADSL.USUBJID",
                        "AVISITN": {"literal": 32},
                        "AVAL": window_derivation(),
                    },
                },
            ],
        }

        with tempfile.TemporaryDirectory() as temp_dir:
            example_dir = Path(temp_dir)
            (example_dir / "adsl.csv").write_text("USUBJID,AVISITN\n01,8\n")
            (example_dir / "qs.csv").write_text("USUBJID,AVISITN,AVAL\n01,8,25\n")
            errors = CONTROLS.document(
                spec, "example/spec.yaml", example_dir / "spec.yaml"
            )
        self.assertEqual(
            [(error.path, error.condition, error.context) for error in errors],
            [
                (
                    "example/spec.yaml.rows[0].filter",
                    "phase_boundary",
                    {
                        "identifier": "AVAL",
                        "row": "direct",
                        "available_phase": "window_derivation",
                        "required_phase": "row_filter",
                    },
                ),
                (
                    "example/spec.yaml.rows[1].filter",
                    "phase_boundary",
                    {
                        "identifier": "DOUBLED",
                        "row": "transitive",
                        "available_phase": "window_derivation",
                        "required_phase": "row_filter",
                    },
                ),
            ],
        )

    def test_an_ungrouped_row_filter_rejects_a_window_derived_lookup_match(self):
        # REQ-0068: a qualified read through a lookup consumes its match
        # values, so a match naming a window-derived template column fails
        # like a direct read. The driver carries AVISITN raw, typed as in
        # QS: row-context joins match raw driver keys (f4308beb), so a
        # driver without the key would be refused on that ground first.
        spec = {
            "domain": "ADQS",
            "input": {
                "ADSL": {"path": "adsl.csv", "types": {"AVISITN": "int"}},
                "QS": {"path": "qs.csv", "types": {"AVISITN": "int", "AVAL": "float"}},
            },
            "keys": ["USUBJID", "AVISITN"],
            "intermediates": [
                {
                    "id": "COLLECTED",
                    "dataset": "QS",
                    "key": ["USUBJID", "AVISITN"],
                    "columns": ["AVAL"],
                    "order_by": ["QS.AVAL"],
                    "keep": "first",
                }
            ],
            "output": {
                "path": "out.csv",
                "columns": ["USUBJID", "AVISITN", "AVAL"],
            },
            "columns": [
                {"name": "USUBJID", "type": "str"},
                {"name": "AVISITN", "type": "int"},
                {"name": "AVAL", "type": "float"},
            ],
            "rows": [
                {
                    "id": "week_8",
                    "dataset": "ADSL",
                    "filter": "COLLECTED.AVAL IS NULL",
                    "derivations": {
                        "USUBJID": "ADSL.USUBJID",
                        "AVISITN": {
                            "row_number": {
                                "window": {
                                    "group_by": ["USUBJID"],
                                    "order_by": ["USUBJID"],
                                },
                            }
                        },
                        "AVAL": "QS.AVAL",
                    },
                },
            ],
        }

        with tempfile.TemporaryDirectory() as temp_dir:
            example_dir = Path(temp_dir)
            (example_dir / "adsl.csv").write_text("USUBJID,AVISITN\n01,8\n")
            (example_dir / "qs.csv").write_text("USUBJID,AVISITN,AVAL\n01,8,25\n")
            errors = CONTROLS.document(
                spec, "example/spec.yaml", example_dir / "spec.yaml"
            )
        self.assertEqual(
            [(error.path, error.condition, error.context) for error in errors],
            [
                (
                    "example/spec.yaml.rows[0].filter",
                    "phase_boundary",
                    {
                        "identifier": "COLLECTED.AVAL",
                        "row": "week_8",
                        "available_phase": "window_derivation",
                        "required_phase": "row_filter",
                        "lookup_match": "AVISITN",
                    },
                )
            ],
        )

    def test_validates_a_grouped_row_count_filter(self):
        spec = {
            "domain": "ADLB",
            "input": {"LB": "lb.csv"},
            "base": "LB",
            "keys": ["USUBJID"],
            "output": {"path": "out.csv", "columns": ["USUBJID"]},
            "columns": [
                {
                    "name": "USUBJID",
                    "type": "str",
                    "derivation": {"source": "LB.USUBJID"},
                },
                {
                    "name": "ABLFL",
                    "type": "str",
                    "derivation": {"literal": "Y"},
                },
            ],
            "verifications": [
                {
                    "row_count": {
                        "id": "one-baseline-per-subject",
                        "group_by": ["USUBJID"],
                        "filter": "ABLFL = 'Y'",
                        "max": 1,
                    }
                },
                {
                    "row_count": {
                        "id": "unknown-operand",
                        "group_by": ["USUBJID"],
                        "filter": "NOSUCHCOL = 'Y'",
                        "max": 1,
                    }
                },
                {
                    "row_count": {
                        "id": "malformed-predicate",
                        "group_by": ["USUBJID"],
                        "filter": "ABLFL =",
                        "max": 1,
                    }
                },
            ],
        }

        prefix = CONTROLS.document(
            spec, "example/spec.yaml", sources={"LB": {"USUBJID": "str"}}
        )
        self.assertEqual(
            [(error.path, error.condition, error.context) for error in prefix],
            [
                (
                    "example/spec.yaml.verifications[1].row_count.filter",
                    "unknown_field",
                    {"identifier": "NOSUCHCOL"},
                )
            ],
        )
        # Reach the independently authored malformed clause without changing
        # native verifier ordering or collecting past a deferred failure.
        independent = copy.deepcopy(spec)
        independent["verifications"][1]["row_count"]["filter"] = "TRUE"
        errors = prefix + CONTROLS.document(
            independent, "example/spec.yaml", sources={"LB": {"USUBJID": "str"}}
        )

        self.assertEqual(len(errors), 2)
        self.assertIn("verifications[1].row_count.filter", errors[0])
        self.assertEqual(errors[0].context, {"identifier": "NOSUCHCOL"})
        self.assertIn("verifications[2].row_count.filter", errors[1])
        self.assertEqual(errors[1].condition, "invalid_predicate")


class TestNumericExpressionLanguage(unittest.TestCase):
    def test_parses_precedence_and_infers_promoted_type(self):
        expression = "A + POWER(B, 2) / -3"
        ast = CONTROLS.numeric_syntax(expression)
        resolver = CONTROLS.catalog(unqualified={"A": "int", "B": "float"})

        path = "spec.columns.RESULT.derivation.compute.expr"
        result_type = CONTROLS.result_type("numeric", expression, path)
        errors = CONTROLS.numeric(expression, path, resolver)

        self.assertEqual(errors, [])
        self.assertEqual(result_type, "float")
        self.assertEqual(ast["span"], (0, len(expression)))

    def test_reports_prohibited_construct_and_function_with_spans(self):
        resolver = CONTROLS.catalog(unqualified={"AVAL": "float", "ANRHI": "float"})

        function_error = CONTROLS.numeric(
            "SUM(AVAL)", "spec.columns.TOTAL.derivation.compute.expr", resolver
        )[0]
        comparison_error = CONTROLS.numeric(
            "AVAL > ANRHI", "spec.columns.FLAG.derivation.compute.expr", resolver
        )[0]
        boolean_error = CONTROLS.numeric(
            "AVAL AND ANRHI", "spec.columns.FLAG.derivation.compute.expr", resolver
        )[0]

        self.assertEqual(function_error.condition, "prohibited_function")
        self.assertEqual(function_error.span, (0, 3))
        self.assertEqual(comparison_error.condition, "prohibited_construct")
        self.assertEqual(comparison_error.span, (5, 6))
        self.assertEqual(boolean_error.condition, "prohibited_construct")
        self.assertEqual(boolean_error.context["construct"], "boolean")

    def test_rejects_unknown_qualified_and_non_numeric_identifiers(self):
        resolver = CONTROLS.catalog(
            unqualified={"TERM": "str"},
            qualified={"PICK": {"VALUE": "float"}},
        )

        unknown = CONTROLS.numeric("MISSING + 1", "spec.compute.expr", resolver)[0]
        qualified = CONTROLS.numeric("DATA.VALUE + 1", "spec.compute.expr", resolver)[0]
        non_numeric = CONTROLS.numeric("TERM + 1", "spec.compute.expr", resolver)[0]
        lookup_errors = CONTROLS.numeric(
            "PICK.VALUE + 1", "spec.compute.expr", resolver
        )

        self.assertEqual(unknown.condition, "unknown_field")
        self.assertEqual(qualified.condition, "qualified_identifier")
        self.assertEqual(non_numeric.condition, "incompatible_input_type")
        self.assertEqual(lookup_errors, [])

    def test_accepts_the_complete_function_vocabulary(self):
        expressions = [
            "ABS(A)",
            "CEIL(A)",
            "FLOOR(A)",
            "TRUNC(A)",
            "SQRT(A)",
            "POWER(A, 2)",
            "EXP(A)",
            "LN(A)",
            "MOD(A, 2)",
            "GREATEST(A, B)",
            "LEAST(A, B)",
            "NULLIF(A, B)",
            "COALESCE(NULL, A, B)",
        ]
        resolver = CONTROLS.catalog(unqualified={"A": "int", "B": "float"})

        for expression in expressions:
            with self.subTest(expression=expression):
                self.assertEqual(
                    CONTROLS.numeric(expression, "spec.compute.expr", resolver),
                    [],
                )

    def test_rejects_prohibited_function_argument_counts(self):
        resolver = CONTROLS.catalog(unqualified={"A": "float"})
        for expression in ("ABS()", "POWER(A)", "GREATEST(A)", "COALESCE()"):
            with self.subTest(expression=expression):
                errors = CONTROLS.numeric(expression, "spec.compute.expr", resolver)
                self.assertEqual(len(errors), 1)
                self.assertEqual(errors[0].condition, "prohibited_function")

    def test_does_not_execute_runtime_failure_conditions(self):
        resolver = CONTROLS.catalog()
        for expression in ("1 / 0", "SQRT(-1)", "LN(0)"):
            with self.subTest(expression=expression):
                errors = CONTROLS.numeric(expression, "spec.compute.expr", resolver)
                self.assertEqual(errors, [])

    def test_dependency_collection_uses_the_parsed_ast(self):
        names = CONTROLS.numeric_names("VALUE + POWER(BASE, 2) + PICK.DOSE")
        self.assertEqual(names, {"VALUE", "BASE", "PICK.DOSE"})


class TestAggregateExpressionLanguage(unittest.TestCase):
    def context(self, **changes):
        context = {
            "kind": "column",
            "input": {
                "EX": {
                    "DOSE": "float",
                    "PLANDOSE": "float",
                    "TERM": "str",
                }
            },
            "output_types": {"VALUE": "float", "TERM": "str"},
            "keys": ["STUDYID", "PLANDOSE"],
        }
        context.update(changes)
        return context

    def test_parses_reducers_functions_and_count_star(self):
        expression = "SUM(EX.DOSE) / NULLIF(EX.PLANDOSE, 0)"
        ast = CONTROLS.aggregate_syntax(expression)
        path = "spec.columns.RDI.derivation.aggregate"
        result_type = CONTROLS.result_type("aggregate", expression, path)
        errors = CONTROLS.aggregate(
            {"expr": expression, "group_by": ["EX.PLANDOSE"]}, path, self.context()
        )
        count_errors = CONTROLS.aggregate(
            "COUNT(EX.*)", "spec.columns.N.derivation.aggregate", self.context()
        )

        self.assertEqual(errors, [])
        self.assertEqual(result_type, "float")
        self.assertEqual(ast["span"], (0, len(expression)))
        self.assertEqual(count_errors, [])
        self.assertEqual(
            CONTROLS.aggregate_names(expression),
            {"EX.DOSE", "EX.PLANDOSE"},
        )

    def test_rejects_invalid_grammar_constructs_and_functions(self):
        path = "spec.columns.X.derivation.aggregate"

        malformed = CONTROLS.aggregate("SUM(EX.DOSE", path, self.context())[0]
        comparison = CONTROLS.aggregate("SUM(EX.DOSE) > 0", path, self.context())[0]
        function = CONTROLS.aggregate("MEDIAN(EX.DOSE)", path, self.context())

        self.assertEqual(malformed.condition, "invalid_aggregate_expression")
        self.assertEqual(comparison.condition, "prohibited_construct")
        self.assertIn(
            "prohibited_function",
            {error.condition for error in function},
        )
        self.assertIsNotNone(malformed.span)

    def test_rejects_nested_reductions_and_mixed_relations(self):
        path = "spec.columns.X.derivation.aggregate"
        nested = CONTROLS.aggregate("MAX(SUM(EX.DOSE))", path, self.context())
        mixed = CONTROLS.aggregate("SUM(EX.DOSE) + SUM(AE.DOSE)", path, self.context())

        self.assertIn("nested_reduction", {error.condition for error in nested})
        self.assertEqual(mixed[0].condition, "mixed_relations")

    def test_enforces_grain_context_and_numeric_reducer_inputs(self):
        path = "spec.columns.X.derivation.aggregate"
        ungrouped = CONTROLS.aggregate(
            "SUM(EX.DOSE) / EX.PLANDOSE", path, self.context()
        )
        non_numeric = CONTROLS.aggregate(
            {"expr": "SUM(TERM)", "group_by": ["VALUE"]},
            path,
            self.context(),
        )
        row_context = self.context(kind="ungrouped_row", driver="EX", row_group_by=None)
        wrong_row_context = CONTROLS.aggregate("SUM(EX.DOSE)", path, row_context)

        self.assertEqual(ungrouped[0].condition, "aggregate_identifier_not_grouped")
        self.assertEqual(ungrouped[0].path, path)
        self.assertEqual(non_numeric[0].condition, "incompatible_input_type")
        self.assertEqual(non_numeric[0].path, f"{path}.expr")
        self.assertEqual(wrong_row_context[0].condition, "invalid_aggregate_context")

    def test_validates_row_relative_range_bounds_and_types(self):
        path = "spec.columns.X.derivation.aggregate"
        missing = CONTROLS.aggregate(
            {"expr": "SUM(EX.DOSE)", "between": {"value": "VALUE"}},
            path,
            self.context(),
        )
        qualified = CONTROLS.aggregate(
            {
                "expr": "SUM(EX.DOSE)",
                "between": {"value": "VALUE", "lower": "EX.DOSE"},
            },
            path,
            self.context(),
        )
        incomparable = CONTROLS.aggregate(
            {
                "expr": "SUM(EX.DOSE)",
                "between": {"value": "TERM", "lower": "DOSE"},
            },
            path,
            self.context(),
        )

        self.assertEqual(
            missing[0].context["reason"], "between declares at least one bound"
        )
        self.assertEqual(qualified[0].condition, "pattern_mismatch")
        self.assertEqual(
            qualified[0].context,
            {"pattern": "^[A-Za-z_][A-Za-z0-9_]*$", "value": "EX.DOSE"},
        )
        self.assertEqual(incomparable[0].condition, "incomparable_range_types")
        self.assertEqual(
            incomparable[0].context, {"value_type": "str", "lower_type": "float"}
        )


class TestAggregateDeriveStep(unittest.TestCase):
    def context(self, **changes):
        datasets = {
            "QS": {"QSORRES": "str", "QSCAT": "str"},
            "EX": {"EXSTDT": "date", "EXSEQ": "int", "EXENDTC": "str"},
        }
        output_types = {"AVAL": "float"}
        aggregate = {
            "kind": "column",
            "input": datasets,
            "output_types": output_types,
            "keys": ["STUDYID", "USUBJID"],
        }
        for key in ("intermediate_ids", "keep_intermediate_ids"):
            if key in changes:
                aggregate[key] = changes.pop(key)
        context = {
            "kind": "column",
            "input": datasets,
            "output_types": output_types,
            "keys": ["STUDYID", "USUBJID"],
            "aggregate": aggregate,
        }
        for key in ("intermediate_ids", "keep_intermediate_ids"):
            if key in aggregate:
                context[key] = aggregate[key]
        context.update(changes)
        if "CAP" in context.get("intermediate_ids", set()):
            datasets["CAP_SOURCE"] = {"CAPDOSE": "float"}
            cap = {"id": "CAP", "dataset": "CAP_SOURCE", "key": ["STUDYID"]}
            if "CAP" in context.get("keep_intermediate_ids", set()):
                cap.update(order_by=["CAP_SOURCE.CAPDOSE"], keep="first")
            context["intermediates"] = [cap]
        return context

    def payload(self, **changes):
        aggregate = {
            "filter": "QS.QSCAT = 'SCALE'",
            "key": ["STUDYID"],
            "derive": [
                {
                    "name": "QSNUM",
                    "type": "float",
                    "derivation": "QS.QSORRES",
                },
            ],
            "expr": "SUM(QSNUM)",
        }
        aggregate.update(changes)
        if aggregate.get("filter") is None:
            aggregate.pop("filter", None)
        return aggregate

    def validate(self, payload, context):
        return CONTROLS.aggregate(
            payload, "spec.columns.AVAL.derivation.aggregate", context
        )

    def test_accepts_derive_bindings_as_reducer_inputs(self):
        errors = self.validate(self.payload(), self.context())
        self.assertEqual(errors, [])

    def test_rejects_windows_in_derive_bindings_at_the_window_path(self):
        path = "spec.columns.AVAL.derivation.aggregate.derive[0].derivation"
        for operation in (
            "row_number",
            "rank",
            "row_value",
            "previous_non_missing",
            "locf",
            "baseline_flag",
        ):
            for qualified in (True, False):
                source = "QS.QSORRES" if qualified else "QSORRES"
                fields = {"window": {"order_by": [source]}}
                if operation in ("row_value", "previous_non_missing", "locf"):
                    fields["source"] = source
                if operation == "row_value":
                    fields["offset"] = -1
                if operation == "baseline_flag":
                    fields.update(date=source, reference_date=source)
                window = {operation: fields}
                for derivation, suffix in (
                    (window, f".{operation}"),
                    (
                        {
                            "case": [
                                {"when": "QS.QSCAT = 'SCALE'", "then": window},
                                {"otherwise": {"literal": 0}},
                            ]
                        },
                        f".case[0].then.{operation}",
                    ),
                    (
                        {
                            "case": [
                                {"when": "QS.QSCAT = 'SCALE'", "then": {"literal": 0}},
                                {"otherwise": window},
                            ]
                        },
                        f".case[1].otherwise.{operation}",
                    ),
                    (
                        {"str_concat": {"sources": [window]}},
                        f".str_concat.sources[0].{operation}",
                    ),
                    ({"value": window, "unconvertible": None}, f".{operation}"),
                ):
                    with self.subTest(
                        operation=operation, qualified=qualified, suffix=suffix
                    ):
                        errors = self.validate(
                            self.payload(
                                derive=[
                                    {
                                        "name": "QSNUM",
                                        "type": "float",
                                        "derivation": derivation,
                                    }
                                ]
                            ),
                            self.context(),
                        )
                        self.assertEqual(len(errors), 1)
                        self.assertEqual(errors[0].condition, "invalid_derive_step")
                        self.assertEqual(errors[0].path, path + suffix)
                        self.assertEqual(
                            errors[0].context["reason"],
                            "a derive binding cannot use a window operation",
                        )

    def test_window_names_in_derive_literals_remain_literals(self):
        derive = self.payload()["derive"] + [
            {
                "name": "TAG",
                "type": "str",
                "derivation": {"literal": "row_value"},
            }
        ]
        self.assertEqual(self.validate(self.payload(derive=derive), self.context()), [])

    def test_rejects_unknown_derive_variable_in_expression(self):
        errors = self.validate(self.payload(expr="SUM(MISSING)"), self.context())
        self.assertEqual(len(errors), 1)
        self.assertEqual(errors[0].condition, "unknown_derive_variable")

    def test_rejects_duplicate_binding_names(self):
        errors = self.validate(
            self.payload(
                derive=[
                    {
                        "name": "QSNUM",
                        "type": "float",
                        "derivation": "QS.QSORRES",
                    },
                    {
                        "name": "QSNUM",
                        "type": "float",
                        "derivation": "QS.QSORRES",
                    },
                ]
            ),
            self.context(),
        )
        self.assertEqual(errors[0].condition, "invalid_derive_step")
        self.assertEqual(
            errors[0].context["reason"],
            "derive binding names are unique identifiers",
        )

    def test_rejects_derive_step_naming_two_relations(self):
        errors = self.validate(
            self.payload(
                derive=[
                    {
                        "name": "A",
                        "type": "float",
                        "derivation": "QS.QSORRES",
                    },
                    {
                        "name": "B",
                        "type": "int",
                        "derivation": "EX.EXSEQ",
                    },
                ],
                expr="SUM(A)",
            ),
            self.context(),
        )
        self.assertEqual(errors[0].condition, "invalid_derive_step")
        self.assertEqual(
            errors[0].context["reason"],
            "a derive step names exactly one relation",
        )

    def test_rejects_unknown_binding_reference(self):
        errors = self.validate(
            self.payload(
                derive=[
                    {
                        "name": "A",
                        "type": "float",
                        "derivation": "MISSING",
                    },
                ],
                expr="SUM(A)",
            ),
            self.context(),
        )
        self.assertEqual(errors[0].condition, "unknown_derive_variable")
        self.assertEqual(errors[0].context["variable"], "MISSING")

    def test_accepts_enum_strings_inside_binding_derivation(self):
        # #732: enum values in a binding derivation are not variable
        # references, so the unknown-variable scan leaves them alone.
        errors = self.validate(
            self.payload(
                filter=None,
                derive=[
                    {
                        "name": "H",
                        "type": "date",
                        "derivation": {
                            "date_impute": {
                                "source": "EX.EXENDTC",
                                "day": "last",
                                "minimum_source_precision": "month",
                                "missing": None,
                            }
                        },
                    },
                ],
                expr="MAX(H)",
            ),
            self.context(),
        )
        self.assertEqual(errors, [])

    def test_rejects_qualified_reference_alongside_bindings(self):
        errors = self.validate(
            self.payload(expr="SUM(QSNUM) + QS.QSORRES"), self.context()
        )
        self.assertEqual(errors[0].condition, "mixed_relations")

    def test_validates_binding_derivation_input_types(self):
        errors = self.validate(
            self.payload(
                filter="EX.EXSEQ = 1",
                derive=[
                    {
                        "name": "EPOCHDAY",
                        "type": "int",
                        "derivation": {
                            "to_epoch_day": {"source": "EX.EXSTDT"},
                        },
                    },
                ],
                expr="ONLY(EPOCHDAY)",
            ),
            self.context(),
        )
        self.assertEqual(errors, [])

    def test_rejects_wrong_temporal_input_inside_binding(self):
        context = self.context()
        context["input"] = {
            "QS": {"QSORRES": "str", "QSCAT": "str"},
            "EX": {"EXSTDT": "str", "EXSEQ": "int"},
        }
        errors = self.validate(
            self.payload(
                filter="EX.EXSEQ = 1",
                derive=[
                    {
                        "name": "EPOCHDAY",
                        "type": "int",
                        "derivation": {
                            "to_epoch_day": {"source": "EX.EXSTDT"},
                        },
                    },
                ],
                expr="ONLY(EPOCHDAY)",
            ),
            context,
        )
        self.assertEqual(
            [error.condition for error in errors],
            ["incompatible_input_type"],
        )
        self.assertEqual(
            errors[0].path,
            "spec.columns.AVAL.derivation.aggregate.derive[0].derivation"
            ".to_epoch_day.source",
        )

    def test_rejects_forward_binding_reference(self):
        errors = self.validate(
            self.payload(
                derive=[
                    {
                        "name": "DOUBLED",
                        "type": "float",
                        "derivation": "QSNUM",
                    },
                    {
                        "name": "QSNUM",
                        "type": "float",
                        "derivation": "QS.QSORRES",
                    },
                ],
                expr="SUM(DOUBLED)",
            ),
            self.context(),
        )
        self.assertEqual(errors[0].condition, "unknown_derive_variable")
        self.assertEqual(errors[0].context["variable"], "QSNUM")

    def test_rejects_self_reference(self):
        errors = self.validate(
            self.payload(
                derive=[
                    {
                        "name": "QSNUM",
                        "type": "float",
                        "derivation": "QSNUM",
                    },
                ],
                expr="SUM(QSNUM)",
            ),
            self.context(),
        )
        self.assertEqual(errors[0].condition, "unknown_derive_variable")
        self.assertEqual(errors[0].context["variable"], "QSNUM")

    def test_rejects_missing_binding_name(self):
        errors = self.validate(
            self.payload(
                derive=[
                    {
                        "type": "float",
                        "derivation": "QS.QSORRES",
                    },
                ],
            ),
            self.context(),
        )
        self.assertEqual(errors[0].condition, "missing_required_field")
        self.assertEqual(errors[0].context["field"], "name")

    def test_rejects_filter_naming_other_relation(self):
        errors = self.validate(
            self.payload(
                filter="EX.EXSEQ > 0",
                derive=[
                    {
                        "name": "A",
                        "type": "float",
                        "derivation": "QS.QSORRES",
                    },
                ],
            ),
            self.context(),
        )
        self.assertEqual(errors[0].condition, "invalid_derive_step")
        self.assertEqual(
            errors[0].context["reason"],
            "a derive step names exactly one relation",
        )

    def test_accepts_keep_declared_intermediate_read(self):
        errors = self.validate(
            self.payload(
                filter="EX.EXSEQ > 0",
                derive=[
                    {
                        "name": "CUM",
                        "type": "float",
                        "derivation": {
                            "compute": {"expr": "EX.EXSEQ + CAP.CAPDOSE"},
                        },
                    },
                ],
                expr="SUM(CUM)",
            ),
            self.context(
                intermediate_ids={"CAP"},
                keep_intermediate_ids={"CAP"},
            ),
        )
        self.assertEqual(errors, [])

    def test_rejects_intermediate_read_without_keep(self):
        errors = self.validate(
            self.payload(
                filter="EX.EXSEQ > 0",
                derive=[
                    {
                        "name": "CUM",
                        "type": "float",
                        "derivation": {
                            "compute": {"expr": "EX.EXSEQ + CAP.CAPDOSE"},
                        },
                    },
                ],
            ),
            self.context(intermediate_ids={"CAP"}),
        )
        self.assertEqual(errors[0].condition, "invalid_derive_step")
        self.assertEqual(
            errors[0].context["reason"],
            "a derive step reads only keep-declared named intermediates",
        )
        self.assertEqual(errors[0].context["intermediate"], "CAP")

    def test_rejects_two_dataset_relations_with_keep_intermediate(self):
        errors = self.validate(
            self.payload(
                derive=[
                    {
                        "name": "A",
                        "type": "float",
                        "derivation": "QS.QSORRES",
                    },
                    {
                        "name": "CAPDOSE",
                        "type": "float",
                        "derivation": "CAP.CAPDOSE",
                    },
                    {
                        "name": "B",
                        "type": "int",
                        "derivation": "EX.EXSEQ",
                    },
                ],
            ),
            self.context(
                intermediate_ids={"CAP"},
                keep_intermediate_ids={"CAP"},
            ),
        )
        self.assertEqual(errors[0].condition, "invalid_derive_step")
        self.assertEqual(
            errors[0].context["reason"],
            "a derive step names exactly one relation",
        )
        self.assertEqual(errors[0].context["relations"], ["EX", "QS"])


class TestStringTemplateLanguage(unittest.TestCase):
    def test_parses_placeholders_and_escaped_braces(self):
        template = "{{{SITEID}}}:{SUBJID}:{ODM.IT.DM.SEX}"
        placeholders = CONTROLS.template_placeholders(template)

        self.assertEqual(
            [placeholder["name"] for placeholder in placeholders],
            ["SITEID", "SUBJID", "ODM.IT.DM.SEX"],
        )
        self.assertEqual(
            CONTROLS.template_names(template),
            {"SITEID", "SUBJID", "ODM.IT.DM.SEX"},
        )

    def test_a_scan_unescapes_brace_pairs_into_literal_text(self):
        # R012 makes the pairs take precedence while scanning, so a template
        # carries the literal text they produce as well as its placeholders.
        self.assertEqual(
            CONTROLS.template_parts("{{{SITEID}}}"),
            [
                {"kind": "text", "value": "{", "span": (0, 2)},
                {"kind": "placeholder", "name": "SITEID", "span": (3, 9)},
                {"kind": "text", "value": "}", "span": (10, 12)},
            ],
        )
        self.assertEqual(CONTROLS.template_parts(""), [])

    def test_rejects_operators_empty_and_unmatched_braces(self):
        for template in ("{A + B}", "{}", "{A", "A}"):
            with self.subTest(template=template):
                errors = CONTROLS.template(
                    template,
                    "spec.columns.X.derivation.str_template",
                    lambda _name: "str",
                )
                self.assertEqual(len(errors), 1)
                self.assertEqual(errors[0].condition, "invalid_string_template")
                self.assertIsNotNone(errors[0].span)

    def test_resolves_placeholder_names_and_types(self):
        types = {"SITEID": "str", "AGE": "int"}
        resolver = CONTROLS.catalog(types)

        valid = CONTROLS.template("{SITEID}", "spec.template", resolver)
        non_string = CONTROLS.template("{AGE}", "spec.template", resolver)
        unknown = CONTROLS.template("{MISSING}", "spec.template", resolver)

        self.assertEqual(valid, [])
        self.assertEqual(non_string[0].condition, "incompatible_input_type")
        self.assertEqual(unknown[0].condition, "unknown_field")


class TestStaticSemanticContracts(unittest.TestCase):
    def context(self):
        output_types = {
            "A": "date",
            "B": "int",
            "DTM": "datetime",
            "KEY1": "str",
            "KEY2": "str",
            "TEXT": "str",
        }
        datasets = {"REF": {"K": "str"}}
        return {
            "resolver": CONTROLS.catalog(unqualified=output_types),
            "input": datasets,
            "aggregate": {
                "kind": "column",
                "input": datasets,
                "output_types": output_types,
                "keys": ["KEY1"],
            },
        }

    def validate(self, expression):
        return CONTROLS.operation(
            expression, "spec.columns.X.derivation", self.context()
        )

    def test_flag_false_value_requires_missing(self):
        # REQ-1258: missing may repeat false_value, name another value, or
        # be null; it may not be absent.
        condition = {"condition": "B > 1"}
        [error] = self.validate({"flag": {**condition, "false_value": "N"}})
        self.assertEqual(error.condition, "missing_value_required")
        self.assertEqual(error.path, "spec.columns.X.derivation.flag.missing")
        self.assertEqual(error.context, {"false_value": "N"})
        # Native issue transport must preserve exact scalar kinds and bytes.
        for false_value in (-(2**63), 2**63 - 1, False, True, "N\x00U"):
            with self.subTest(false_value=false_value):
                [exact] = self.validate(
                    {
                        "flag": {
                            **condition,
                            "false_value": false_value,
                        }
                    }
                )
                self.assertEqual(exact.condition, "missing_value_required")
                self.assertEqual(exact.context, {"false_value": false_value})
                self.assertIs(type(exact.context["false_value"]), type(false_value))
        for missing in ("N", "U", None):
            with self.subTest(missing=missing):
                self.assertEqual(
                    self.validate(
                        {
                            "flag": {
                                **condition,
                                "false_value": "N",
                                "missing": missing,
                            }
                        }
                    ),
                    [],
                )
        for flag in ("B > 1", condition, {**condition, "missing": "U"}):
            with self.subTest(flag=flag):
                self.assertEqual(self.validate({"flag": flag}), [])

    def test_window_order_by_required(self):
        for operation in ("row_number", "rank", "row_value", "previous_non_missing"):
            payload = (
                {"source": "B", "offset": 1}
                if operation == "row_value"
                else {"source": "B"}
                if operation == "previous_non_missing"
                else {}
            )
            errors = self.validate({operation: payload})
            self.assertEqual(len(errors), 1)
            self.assertEqual(errors[0].condition, "window_order_by_required")
            self.assertEqual(errors[0].context, {"operation": operation})
            self.assertTrue(errors[0].path.endswith(f".{operation}.window"))

            ordered = self.validate(
                {operation: {**payload, "window": {"order_by": ["B"]}}}
            )
            self.assertEqual(ordered, [])

    def test_window_order_by_forbidden_on_baseline_flag(self):
        errors = self.validate(
            {
                "baseline_flag": {
                    "date": "A",
                    "reference_date": "A",
                    "window": {"order_by": ["B"]},
                }
            }
        )
        self.assertEqual(len(errors), 1)
        self.assertEqual(errors[0].condition, "window_order_by_forbidden")

        unordered = self.validate(
            {"baseline_flag": {"date": "A", "reference_date": "A"}}
        )
        self.assertEqual(unordered, [])

    def test_mapping_extreme_window_and_cut_contracts(self):
        collision = self.validate(
            {
                "mapping": {
                    "source": "TEXT",
                    "case_sensitive": False,
                    "dict": {"Y": "Y", "y": "Y"},
                }
            }
        )
        extreme = self.validate({"greatest": {"sources": ["A", "B"]}})
        offset = self.validate(
            {"row_value": {"source": "B", "offset": 0, "window": {"order_by": ["B"]}}}
        )
        cut = self.validate(
            {"cut": {"source": "B", "breaks": [10, 5], "labels": ["a"]}}
        )

        self.assertEqual(collision[0].condition, "ambiguous_dictionary")
        self.assertEqual(extreme[0].condition, "incomparable_sources")
        self.assertEqual(offset[0].condition, "zero_offset")
        self.assertEqual(
            [(error.condition, error.context) for error in cut],
            [
                (
                    "invalid_field_type",
                    {"expected": "one label more than breaks", "operation": "cut"},
                ),
                (
                    "invalid_field_type",
                    {"expected": "ascending breaks", "operation": "cut"},
                ),
            ],
        )

    def test_rejects_unknown_direct_operation_inputs(self):
        context = self.context()

        errors = CONTROLS.operation(
            {"source": "MISSING"},
            "spec.columns.X.derivation",
            context,
        )

        self.assertEqual(len(errors), 1)
        self.assertEqual(errors[0].condition, "unknown_field")

    def test_temporal_literal_ranges_and_input_types(self):
        impute = self.validate(
            {"date_impute": {"source": "TEXT", "month": 13, "day": 0}}
        )
        valid_to_date = self.validate({"to_date": {"source": "DTM"}})
        invalid_to_date = self.validate({"to_date": {"source": "A"}})
        valid_epoch_day = self.validate({"to_epoch_day": {"source": "A"}})
        invalid_epoch_day = self.validate({"to_epoch_day": {"source": "TEXT"}})

        self.assertEqual(
            [(error.condition, error.context) for error in impute],
            [
                ("month_out_of_range", {"month": 13}),
                (
                    "value_not_permitted",
                    {"field": "day", "permitted": "1 to 31", "value": 0},
                ),
            ],
        )
        self.assertEqual(valid_to_date, [])
        self.assertEqual(invalid_to_date[0].condition, "incompatible_input_type")
        self.assertEqual(valid_epoch_day, [])
        self.assertEqual(invalid_epoch_day[0].condition, "incompatible_input_type")

    def test_intermediate_range_types(self):
        spec = {
            "intermediates": [
                {
                    "id": "R",
                    "dataset": "REF",
                    "between": {"value": "A", "lower": "LO", "upper": "HI"},
                }
            ]
        }
        errors = CONTROLS.intermediate(
            spec,
            "spec.yaml",
            {"REF": {"LO": "int", "HI": "float"}},
            {"A": "date"},
        )

        self.assertEqual(len(errors), 1)
        self.assertEqual(errors[0].condition, "incomparable_range_types")

    def test_only_a_null_no_match_beside_id_and_dataset_is_a_rename(self):
        # REQ-1248: {id, dataset, no_match: null} reads what the implicit
        # join reads; without no_match the intermediate requires a match.
        def errors(intermediate):
            return CONTROLS.intermediate(
                {"intermediates": [intermediate]},
                "spec.yaml",
                {"REF": {"K": "str"}},
                {},
            )

        [renamed] = errors({"id": "R", "dataset": "REF", "no_match": None})
        self.assertEqual(renamed.condition, "rename_only_intermediate")
        self.assertEqual(errors({"id": "R", "dataset": "REF"}), [])
        self.assertEqual(errors({"id": "R", "dataset": "REF", "no_match": "NA"}), [])

    def test_intermediate_equality_key_types(self):
        spec = {
            "intermediates": [
                {
                    "id": "R",
                    "dataset": "REF",
                    "key": {"K": "A"},
                }
            ]
        }
        errors = CONTROLS.intermediate(
            spec,
            "spec.yaml",
            {"REF": {"K": "str"}},
            {"A": "date"},
        )

        self.assertEqual(len(errors), 1)
        self.assertEqual(errors[0].condition, "incompatible_input_type")

    @staticmethod
    def self_intermediate_spec(prior_filter):
        # The second row template reads the first one's completed rows.
        return {
            "domain": "ADAE",
            "input": {
                "AE": {"path": "ae.csv", "types": {"AESEQ": "int"}},
            },
            "keys": ["USUBJID", "AESEQ"],
            "output": {
                "path": "adae.csv",
                "columns": ["USUBJID", "AESEQ", "PRIOR", "GAP"],
            },
            "intermediates": [
                {
                    "id": "PRIOR_SAE",
                    "dataset": "SELF",
                    "key": ["USUBJID"],
                    "filter": prior_filter,
                    "order_by": [{"variable": "SELF.AESEQ", "direction": "desc"}],
                    "keep": "first",
                }
            ],
            "columns": [
                {"name": "USUBJID", "type": "str"},
                {"name": "AESEQ", "type": "int"},
                {"name": "PRIOR", "type": "int"},
                {"name": "GAP", "type": "int"},
            ],
            "rows": [
                {
                    "id": "first",
                    "dataset": "AE",
                    "derivations": {
                        "USUBJID": "AE.USUBJID",
                        "AESEQ": "AE.AESEQ",
                        "PRIOR": {"literal": None},
                        "GAP": {"literal": None},
                    },
                },
                {
                    "id": "second",
                    "dataset": "AE",
                    "derivations": {
                        "USUBJID": "AE.USUBJID",
                        "AESEQ": "AE.AESEQ",
                        "PRIOR": "PRIOR_SAE.AESEQ",
                        "GAP": {"compute": {"expr": "AE.AESEQ - PRIOR_SAE.AESEQ"}},
                    },
                },
            ],
        }

    def self_intermediate_errors(self, spec):
        with tempfile.TemporaryDirectory() as temp_dir:
            example_dir = Path(temp_dir)
            (example_dir / "ae.csv").write_text("USUBJID,AESEQ\n01,1\n")
            spec_path = example_dir / "spec.yaml"
            label = "example/spec.yaml"
            return CONTROLS.document(spec, label, spec_path)

    def test_self_intermediate_resolves_output_columns(self):
        # REQ-0120: a SELF intermediate reads completed output rows, so its
        # dataset needs no input declaration and its donor fields resolve
        # against the output columns, bare or SELF-qualified in its filter.
        # REQ-0125: row derivations read it through its id, in predicates
        # and numeric expressions alike.
        for prior_filter in (
            "SELF.AESEQ < AE.AESEQ",
            "AESEQ < AE.AESEQ",
        ):
            with self.subTest(prior_filter=prior_filter):
                spec = self.self_intermediate_spec(prior_filter)
                self.assertEqual(self.self_intermediate_errors(spec), [])

    def test_self_intermediate_reads_hoisted_template_columns(self):
        # REQ-1260: a derivation every template shares is written once at
        # column level. It is row-local and a SELF read names it, so each
        # template inherits it as a row-phase default and it stays a donor
        # field: in the filter, in order_by, and through the intermediate.
        spec = self.self_intermediate_spec("SELF.AESEQ < AE.AESEQ")
        for column in spec["columns"]:
            if column["name"] in ("USUBJID", "AESEQ"):
                column["derivation"] = f"AE.{column['name']}"
        for row in spec["rows"]:
            del row["derivations"]["USUBJID"], row["derivations"]["AESEQ"]

        self.assertEqual(self.self_intermediate_errors(spec), [])

    def test_self_intermediate_reads_row_local_column_defaults(self):
        # REQ-1260: a row-local column-level derivation that a SELF read
        # names is a donor field, bare or SELF-qualified. Reading another
        # row-local column-level column keeps it row-local.
        cases = {
            "source": [
                {"name": "LATE", "type": "int", "derivation": "AE.AESEQ"},
            ],
            "chain": [
                {"name": "BASE", "type": "int", "derivation": "AE.AESEQ"},
                {
                    "name": "LATE",
                    "type": "int",
                    "derivation": {"compute": {"expr": "BASE + 1"}},
                },
            ],
        }
        for label, columns in cases.items():
            for late in ("SELF.LATE", "LATE"):
                with self.subTest(case=label, late=late):
                    spec = self.self_intermediate_spec(
                        f"SELF.AESEQ < AE.AESEQ AND {late} > 0"
                    )
                    spec["columns"].extend(columns)
                    self.assertEqual(self.self_intermediate_errors(spec), [])

    def test_self_intermediate_excludes_column_phase_columns(self):
        # REQ-0120/REQ-1260: a column-level derivation that is not row-local
        # keeps its column phase, so it is not on the completed rows a SELF
        # intermediate reads, whether it uses a window, reads a SELF-backed
        # intermediate, or reads a column that is not row-local.
        window = {
            "row_number": {"window": {"group_by": ["USUBJID"], "order_by": ["AESEQ"]}}
        }
        cases = {
            "window": [
                {"name": "LATE", "type": "int", "derivation": window},
            ],
            "self_intermediate": [
                {"name": "LATE", "type": "int", "derivation": "FIRST_AE.AESEQ"},
            ],
            "transitive": [
                {"name": "RANKED", "type": "int", "derivation": window},
                {
                    "name": "LATE",
                    "type": "int",
                    "derivation": {"compute": {"expr": "RANKED + 0"}},
                },
            ],
        }
        for label, columns in cases.items():
            with self.subTest(case=label):
                spec = self.self_intermediate_spec(
                    "SELF.AESEQ < AE.AESEQ AND SELF.LATE > 0"
                )
                dataset = "SELF" if label == "self_intermediate" else "AE"
                spec["intermediates"].append(
                    {
                        "id": "FIRST_AE",
                        "dataset": dataset,
                        "key": ["USUBJID"],
                        "order_by": [f"{dataset}.AESEQ"],
                        "keep": "first",
                    }
                )
                spec["columns"].extend(columns)

                errors = self.self_intermediate_errors(spec)

                self.assertEqual(
                    [(error.path, error.condition, error.context) for error in errors],
                    [
                        (
                            "example/spec.yaml.intermediates[0].filter",
                            "unknown_field",
                            {"identifier": "SELF.LATE", "intermediate": "PRIOR_SAE"},
                        ),
                        (
                            "example/spec.yaml.rows[1].derivations.PRIOR.source",
                            "unknown_field",
                            {"identifier": "PRIOR_SAE.AESEQ"},
                        ),
                        (
                            "example/spec.yaml.rows[1].derivations.GAP.compute.expr",
                            "unknown_field",
                            {"identifier": "PRIOR_SAE.AESEQ"},
                        ),
                    ],
                )

    def test_self_intermediate_reads_input_backed_named_column_defaults(self):
        # REQ-1260 admits input-backed named reads as row defaults. Their
        # transitive readers are available in completed SELF donors too.
        for transitive in (False, True):
            with self.subTest(transitive=transitive):
                spec = self.self_intermediate_spec(
                    "SELF.AESEQ < AE.AESEQ AND SELF.LATE > 0"
                )
                spec["intermediates"].append(
                    {
                        "id": "FIRST_AE",
                        "dataset": "AE",
                        "key": ["USUBJID"],
                        "order_by": ["AE.AESEQ"],
                        "keep": "first",
                    }
                )
                spec["columns"].append(
                    {
                        "name": "BASE" if transitive else "LATE",
                        "type": "int",
                        "derivation": "FIRST_AE.AESEQ",
                    }
                )
                if transitive:
                    spec["columns"].append(
                        {
                            "name": "LATE",
                            "type": "int",
                            "derivation": {"compute": {"expr": "BASE + 1"}},
                        }
                    )
                self.assertEqual(self.self_intermediate_errors(spec), [])

    def test_self_intermediate_requires_row_templates(self):
        # REQ-0120: SELF names completed rows only when the spec declares
        # row templates; without them it is an undeclared dataset.
        spec = self.self_intermediate_spec("SELF.AESEQ < AE.AESEQ")
        del spec["rows"]

        errors = self.self_intermediate_errors(spec)
        self.assertEqual(
            [(error.path, error.condition, error.context) for error in errors],
            [
                (
                    "example/spec.yaml.intermediates[0].dataset",
                    "unknown_field",
                    {"identifier": "SELF", "intermediate": "PRIOR_SAE"},
                )
            ],
        )

    def test_self_intermediate_rejects_declared_self_input(self):
        # REQ-0120: SELF is reserved for the completed rows, so it cannot
        # also name an input dataset.
        spec = self.self_intermediate_spec("SELF.AESEQ < AE.AESEQ")
        spec["input"]["SELF"] = {
            "path": "self.csv",
            "types": {"AESEQ": "int"},
        }

        errors = self.self_intermediate_errors(spec)
        self.assertEqual(
            [(error.path, error.condition, error.context) for error in errors],
            [
                (
                    "example/spec.yaml.intermediates[0].dataset",
                    "duplicate_identifier",
                    {"identifier": "SELF"},
                ),
                (
                    "example/spec.yaml.input.SELF",
                    "duplicate_identifier",
                    {"identifier": "SELF"},
                ),
            ],
        )

    def test_dependency_cycle_reports_each_participating_derivation(self):
        spec = {
            "columns": [
                {
                    "name": "A",
                    "type": "float",
                    "derivation": {"first_available": {"sources": ["B"]}},
                },
                {
                    "name": "B",
                    "type": "float",
                    "derivation": {
                        "row_value": {
                            "source": "A",
                            "offset": -1,
                            "window": {"order_by": ["A"]},
                        }
                    },
                },
            ]
        }

        errors = self.dependency_errors(spec)

        self.assertEqual(
            self.dependency_facts(errors),
            [
                (
                    "validation",
                    "dependency_cycle",
                    "REQ-0072",
                    "example/spec.yaml.columns.A.derivation.first_available",
                    {"cycle": ["A", "B", "A"]},
                ),
                (
                    "validation",
                    "dependency_cycle",
                    "REQ-0072",
                    "example/spec.yaml.columns.B.derivation.row_value",
                    {"cycle": ["A", "B", "A"]},
                ),
            ],
        )

    def test_forward_reference_reports_the_later_column(self):
        spec = {
            "columns": [
                {
                    "name": "A",
                    "type": "float",
                    "derivation": {"first_available": {"sources": ["B"]}},
                },
                {
                    "name": "B",
                    "type": "float",
                    "derivation": {"literal": 1.5},
                },
            ]
        }

        self.assertEqual(
            self.dependency_facts(self.dependency_errors(spec)),
            [
                (
                    "validation",
                    "forward_reference",
                    "REQ-0071",
                    "example/spec.yaml.columns.A.derivation.first_available",
                    {"column": "A", "dependency": "B"},
                ),
            ],
        )

    def test_forward_reference_skips_edges_inside_a_cycle(self):
        spec = {
            "columns": [
                {
                    "name": "A",
                    "type": "float",
                    "derivation": {"first_available": {"sources": ["B"]}},
                },
                {
                    "name": "B",
                    "type": "float",
                    "derivation": {
                        "row_value": {
                            "source": "A",
                            "offset": -1,
                            "window": {"order_by": ["A"]},
                        }
                    },
                },
            ]
        }

        errors = self.dependency_errors(spec)
        self.assertEqual(
            [error.condition for error in errors],
            ["dependency_cycle", "dependency_cycle"],
        )

    def dependency_errors(self, spec, *, source="ID,V\n1,1.5\n"):
        """Admit complete authored files with the installed native transport."""
        columns = [
            {"name": "ID", "type": "int", "derivation": "SRC.ID"},
            *copy.deepcopy(spec["columns"]),
        ]
        for column in columns:
            column["label"] = column["name"]
        complete = {
            "schema_version": "1.0",
            "domain": "TEST",
            "input": {
                "SRC": {"path": "source.csv", "types": {"ID": "int", "V": "float"}}
            },
            "keys": ["ID"],
            "columns": columns,
            "output": {
                "path": "result.csv",
                "columns": [column["name"] for column in columns],
            },
        }
        complete.update({key: value for key, value in spec.items() if key != "columns"})
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path = root / "spec.yaml"
            path.write_text(yaml.safe_dump(complete), encoding="ascii")
            if source is not None:
                (root / "source.csv").write_text(source, encoding="ascii")
            errors = OWNER_REPOSITORY_SCHEMA.validate_spec_document(
                complete,
                "example/spec.yaml",
                path,
                {"root": str(ROOT)},
            )
            self.assertFalse((root / "result.csv").exists())
            return errors

    @staticmethod
    def dependency_facts(errors):
        return [
            (error.phase, error.condition, error.requirement, error.path, error.context)
            for error in errors
        ]

    def test_key_dependency_preserves_complete_native_order_and_paths(self):
        errors = self.dependency_errors(
            {
                "columns": [
                    {"name": "VALUE", "type": "float", "derivation": {"literal": 1.5}},
                ],
                "keys": ["VALUE"],
            }
        )
        self.assertEqual(errors, [])
        errors = self.dependency_errors(
            {
                "columns": [
                    {
                        "name": "KEY",
                        "type": "float",
                        "derivation": {"compute": {"expr": "VALUE"}},
                    },
                    {"name": "VALUE", "type": "float", "derivation": {"literal": 1.5}},
                ],
                "keys": ["ID", "KEY"],
            }
        )
        self.assertEqual(
            self.dependency_facts(errors),
            [
                (
                    "validation",
                    "forward_reference",
                    "REQ-0071",
                    "example/spec.yaml.columns.KEY.derivation.compute",
                    {"column": "KEY", "dependency": "VALUE"},
                ),
                (
                    "validation",
                    "key_dependency",
                    "REQ-0074",
                    "example/spec.yaml.columns.KEY.derivation",
                    {"column": "KEY", "dependency": "VALUE"},
                ),
            ],
        )

    def test_completed_dependencies_are_admitted_without_publication(self):
        self.assertEqual(
            self.dependency_errors(
                {
                    "columns": [
                        {"name": "B", "type": "float", "derivation": {"literal": 1.5}},
                        {
                            "name": "A",
                            "type": "float",
                            "derivation": {"first_available": {"sources": ["B"]}},
                        },
                    ]
                }
            ),
            [],
        )

    def test_header_binding_failure_precedes_dependency_admission(self):
        errors = self.dependency_errors(
            {
                "columns": [
                    {
                        "name": "A",
                        "type": "float",
                        "derivation": {"first_available": {"sources": ["B"]}},
                    },
                    {"name": "B", "type": "float", "derivation": {"literal": 1.5}},
                ]
            },
            source="WRONG,V\n1,1.5\n",
        )
        self.assertEqual(
            self.dependency_facts(errors),
            [
                (
                    "validation",
                    "unknown_field",
                    "REQ-0532",
                    "example/spec.yaml.input.SRC.types.ID",
                    {"dataset": "SRC", "field": "ID"},
                ),
            ],
        )

    def test_resource_path_failure_precedes_dependency_admission(self):
        errors = self.dependency_errors(
            {
                "columns": [
                    {
                        "name": "A",
                        "type": "float",
                        "derivation": {"first_available": {"sources": ["B"]}},
                    },
                    {"name": "B", "type": "float", "derivation": {"literal": 1.5}},
                ]
            },
            source=None,
        )
        self.assertEqual(
            self.dependency_facts(errors),
            [
                (
                    "validation",
                    "resource_path_missing",
                    "REQ-0785",
                    "example/spec.yaml.input.SRC.path",
                    {"dataset": "SRC", "path": "source.csv"},
                ),
            ],
        )

    def test_dependency_transport_failure_is_explicit(self):
        with (
            patch.object(
                OWNER_NATIVE_SPECIFICATION,
                "validate",
                side_effect=RuntimeError("native transport unavailable"),
            ),
            self.assertRaisesRegex(RuntimeError, "native transport unavailable"),
        ):
            self.dependency_errors(
                {
                    "columns": [
                        {"name": "A", "type": "float", "derivation": {"literal": 1.5}},
                    ]
                }
            )

    def test_missing_native_installation_cannot_import_semantic_transport(self):
        with (
            patch.dict(sys.modules, {"yamaa._native": None}),
            self.assertRaisesRegex(ModuleNotFoundError, "yamaa._native"),
        ):
            importlib.reload(OWNER_NATIVE_SPECIFICATION)


class TestConditionRegistry(unittest.TestCase):
    def setUp(self):
        self.root = ROOT
        self.registry, load_errors = OWNER_REPOSITORY_MANIFESTS.load_condition_registry(
            self.root
        )
        self.assertEqual(load_errors, [])

    def test_repository_registry_passes(self):
        self.assertEqual(
            OWNER_REPOSITORY_MANIFESTS.validate_condition_registry(
                self.root, self.registry
            ),
            [],
        )

    def test_unregistered_condition_is_rejected(self):
        changed = copy.deepcopy(self.registry)
        del changed["conditions"]["aggregate_multiple_records"]

        errors = OWNER_REPOSITORY_MANIFESTS.validate_condition_registry(
            self.root, changed
        )

        self.assertEqual(
            errors,
            [
                "ERROR: benchmarks/negative-adlb-duplicate-wbc/"
                "expected/error.yaml.condition: unregistered condition "
                "'aggregate_multiple_records'",
                "ERROR: benchmarks/negative-multiple-baselines/"
                "expected/error.yaml.condition: unregistered condition "
                "'aggregate_multiple_records'",
            ],
        )

    def test_wrong_phase_condition_is_rejected(self):
        changed = copy.deepcopy(self.registry)
        changed["conditions"]["aggregate_multiple_records"]["phases"] = ["derivation"]

        errors = OWNER_REPOSITORY_MANIFESTS.validate_condition_registry(
            self.root, changed
        )

        self.assertEqual(
            errors,
            [
                "ERROR: benchmarks/negative-adlb-duplicate-wbc/"
                "expected/error.yaml.phase: condition "
                "'aggregate_multiple_records' is not registered for phase "
                "'row_construction'"
            ],
        )


class TestValidationMetadata(unittest.TestCase):
    def test_repository_metadata_is_complete_and_registered(self):
        fixtures, errors = OWNER_REPOSITORY_MANIFESTS.load_validation_fixtures(ROOT)
        self.assertEqual(errors, [])
        self.assertEqual(
            set(fixtures),
            set(OWNER_REPOSITORY_MANIFESTS.validation_phase_contracts(ROOT)),
        )

    def test_metadata_rejects_missing_stale_and_unregistered_declarations(self):
        source = ROOT / "benchmarks/negative-unknown-window"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            case = root / "benchmarks" / source.name
            shutil.copytree(source, case)
            metadata_path = case / "benchmark.yaml"
            metadata = yaml.safe_load(metadata_path.read_text())
            original = copy.deepcopy(metadata)
            del metadata["validation"]
            metadata_path.write_text(yaml.safe_dump(metadata))
            _, errors = OWNER_REPOSITORY_MANIFESTS.load_validation_fixtures(root)
            self.assertTrue(
                any("missing validation declaration" in error for error in errors)
            )
            metadata_path.write_text(yaml.safe_dump(original))
            contract_path = case / "expected/error.yaml"
            contract = yaml.safe_load(contract_path.read_text())
            contract["condition"] = "not_registered"
            contract_path.write_text(yaml.safe_dump(contract))
            _, errors = OWNER_REPOSITORY_MANIFESTS.load_validation_fixtures(root)
            self.assertTrue(
                any("unregistered validation condition" in error for error in errors)
            )
            contract["phase"] = "derivation"
            contract_path.write_text(yaml.safe_dump(contract))
            _, errors = OWNER_REPOSITORY_MANIFESTS.load_validation_fixtures(root)
            self.assertTrue(
                any("stale or non-validation fixture" in error for error in errors)
            )

    def test_implemented_fixture_requires_every_declared_path(self):
        entry = {
            "condition": "duplicate_identifier",
            "spec_paths": ["input.ADLB", "domain"],
            "validator": "name_contract",
        }
        diagnostics = [
            OWNER_VALIDATION_DIAGNOSTICS.validation_diagnostic(
                "example/spec.yaml.input.ADLB",
                "duplicate_identifier",
                "duplicate",
            )
        ]

        errors = OWNER_REPOSITORY_MANIFESTS.validate_registered_fixture_diagnostics(
            "negative-example", entry, diagnostics, ["example/spec.yaml"]
        )

        self.assertEqual(len(errors), 1)
        self.assertIn("'domain'", errors[0])

    def test_blocked_fixture_fails_when_every_path_is_implemented(self):
        entry = {
            "condition": "dependency_cycle",
            "spec_paths": ["columns.A", "columns.B"],
            "blocked_by": "#103",
        }
        diagnostics = [
            OWNER_VALIDATION_DIAGNOSTICS.validation_diagnostic(
                f"example/spec.yaml.{path}", "dependency_cycle", "cycle"
            )
            for path in entry["spec_paths"]
        ]

        errors = OWNER_REPOSITORY_MANIFESTS.validate_registered_fixture_diagnostics(
            "negative-example", entry, diagnostics, ["example/spec.yaml"]
        )

        self.assertEqual(len(errors), 1)
        self.assertIn("stale block", errors[0])

    def test_unrelated_diagnostic_is_not_suppressed(self):
        entry = {
            "condition": "unknown_field",
            "spec_paths": ["columns.RESULT.derivation.compute.expr"],
            "validator": "numeric_expression",
        }
        expected = OWNER_VALIDATION_DIAGNOSTICS.validation_diagnostic(
            "example/spec.yaml.columns.RESULT.derivation.compute.expr",
            "unknown_field",
            "unknown identifier",
        )
        unrelated = OWNER_VALIDATION_DIAGNOSTICS.validation_diagnostic(
            "example/spec.yaml.output.columns",
            "undeclared_column",
            "unknown output column",
        )

        errors = OWNER_REPOSITORY_MANIFESTS.validate_registered_fixture_diagnostics(
            "negative-example", entry, [expected, unrelated], ["example/spec.yaml"]
        )

        self.assertEqual(errors, [unrelated])


class TestProjectFunctionEnvironment(unittest.TestCase):
    def test_submission_environment_schema_is_checked_without_domain_or_data_reads(
        self,
    ):
        real_root = ROOT
        schema, errors = OWNER_REPOSITORY_SCHEMA.build_schema_env(
            real_root, "schema_environment.yaml"
        )
        self.assertEqual(errors, [])
        original = yaml.safe_load(
            (real_root / "benchmarks/sdtm-dm-metadata/environment.yaml").read_text()
        )
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            case = root / "benchmarks" / "submission"
            case.mkdir(parents=True)
            environment = case / "environment.yaml"
            environment.write_text(yaml.safe_dump(original))
            with patch.object(OWNER_NATIVE_SPECIFICATION, "validate") as native:
                self.assertEqual(
                    OWNER_REPOSITORY_STRUCTURE.validate_examples_environment_documents(
                        root, schema
                    ),
                    [],
                )
                for value in ("", 1, [], None):
                    original["sdtm"]["source_system_version"] = value
                    environment.write_text(yaml.safe_dump(original))
                    errors = OWNER_REPOSITORY_STRUCTURE.validate_examples_environment_documents(
                        root, schema
                    )
                    self.assertTrue(errors, repr(value))
                    self.assertTrue(
                        any("source_system_version" in error for error in errors)
                    )
            native.assert_not_called()
            self.assertFalse((case / "spec.yaml").exists())
            self.assertFalse((case / "dm.csv").exists())

    def test_metadata_only_specification_does_not_enter_domain_function_bridge(self):
        with tempfile.TemporaryDirectory() as temporary:
            case = Path(temporary) / "benchmarks" / "submission"
            case.mkdir(parents=True)
            (case / "environment.yaml").write_text('schema_version: "1.0"\n')
            specification = {
                "submission": {"label": "Demographics"},
                "columns": [
                    {
                        "name": "STUDYID",
                        "type": "str",
                        "submission": {"core": "Req", "role": "Identifier"},
                    }
                ],
            }
            with patch.object(OWNER_NATIVE_SPECIFICATION, "validate") as native:
                errors = OWNER_REPOSITORY_SCHEMA.validate_spec_functions(
                    specification, "submission.spec", case / "spec.yaml", {}
                )
            self.assertEqual(errors, [])
            native.assert_not_called()

    def test_intermediate_function_calls_retain_native_static_admission(self):
        with tempfile.TemporaryDirectory() as temporary:
            case = Path(temporary) / "benchmarks" / "project"
            case.mkdir(parents=True)
            (case / "environment.yaml").write_text('schema_version: "1.0"\n')
            (case / "spec.yaml").write_text('schema_version: "1.0"\n')
            specification = {
                "intermediates": [
                    {
                        "id": "derived",
                        "derivations": {"value": {"function": {"name": "missing"}}},
                    }
                ]
            }
            with patch.object(
                OWNER_NATIVE_SPECIFICATION,
                "validate",
                return_value=[
                    (
                        "validation",
                        "unknown_function",
                        "REQ-0675",
                        ["intermediates[0].derivations.value"],
                        '{"function":"missing"}',
                    ),
                ],
            ) as native:
                errors = OWNER_REPOSITORY_SCHEMA.validate_spec_functions(
                    specification, "project.spec", case / "spec.yaml", {}
                )
            native.assert_called_once()
            self.assertEqual(len(errors), 1)
            self.assertIn("unknown_function", errors[0])

    def test_invalid_authored_language_reaches_native_schema_diagnostics(self):
        for language in (7, "julia", []):
            with (
                self.subTest(language=language),
                tempfile.TemporaryDirectory() as temporary,
            ):
                case = Path(temporary) / "benchmarks" / "project"
                case.mkdir(parents=True)
                (case / "spec.yaml").write_text('schema_version: "1.0"\n')
                environment = case / "environment.yaml"
                environment.write_text(
                    yaml.safe_dump({"schema_version": "1.0", "language": language})
                )
                errors = OWNER_REPOSITORY_SCHEMA.validate_spec_functions_against(
                    {}, "project.spec", case / "spec.yaml", {}, [], {}, environment
                )
                self.assertTrue(errors)
                self.assertTrue(any("language" in str(error) for error in errors))

    def test_malformed_environment_is_reported_without_entering_native_check(self):
        for content in (
            b"language: [",
            b"language: r\nlanguage: python\n",
            b"- r\n",
            b"\xff",
        ):
            with (
                self.subTest(content=content),
                tempfile.TemporaryDirectory() as temporary,
            ):
                case = Path(temporary) / "benchmarks" / "project"
                case.mkdir(parents=True)
                environment = case / "environment.yaml"
                environment.write_bytes(content)
                with patch.object(OWNER_NATIVE_SPECIFICATION, "validate") as native:
                    errors = OWNER_REPOSITORY_SCHEMA.validate_spec_functions_against(
                        {}, "project.spec", case / "spec.yaml", {}, [], {}, environment
                    )
                self.assertEqual(len(errors), 1)
                self.assertIn(str(environment), str(errors[0]))
                native.assert_not_called()

    def test_pathless_native_rejections_are_reported_at_the_environment(self):
        with tempfile.TemporaryDirectory() as temporary:
            case = Path(temporary) / "benchmarks" / "project"
            case.mkdir(parents=True)
            (case / "spec.yaml").write_text('schema_version: "1.0"\n')
            environment = case / "environment.yaml"
            environment.write_text("language: r\n")
            with patch.object(
                OWNER_NATIVE_SPECIFICATION,
                "validate",
                return_value=[
                    (
                        "validation",
                        "engine_rejected",
                        None,
                        [],
                        '{"stage":"environment","code":"invalid_packaging_lock"}',
                    ),
                ],
            ):
                errors = OWNER_REPOSITORY_SCHEMA.validate_spec_functions_against(
                    {}, "project.spec", case / "spec.yaml", {}, [], {}, environment
                )
            self.assertEqual(len(errors), 1)
            self.assertIn(str(environment), str(errors[0]))
            self.assertIn("engine_rejected", str(errors[0]))

    def test_authoritative_versionless_environment_and_call_shapes(self):
        root = ROOT
        environment, errors = OWNER_REPOSITORY_SCHEMA.build_schema_env(
            root, "schema_environment.yaml"
        )
        self.assertEqual(errors, [])
        source = yaml.safe_load(
            (root / "benchmarks/adam-adsl-bmi/python/environment.yaml").read_text()
        )
        self.assertEqual(
            OWNER_REPOSITORY_SCHEMA.validate_type(
                source, ["environment_class"], environment, "environment"
            ),
            [],
        )
        source["version"] = "1.0.0"
        self.assertTrue(
            OWNER_REPOSITORY_SCHEMA.validate_type(
                source, ["environment_class"], environment, "environment"
            )
        )

    def test_original_function_studies_use_explicit_equivalent_environments(self):
        root = ROOT
        for name in (
            "adam-adsl-bmi",
            "adam-advs-percentiles",
            "schema-functions",
            "schema-non-finite",
            "negative-function-contract",
        ):
            with self.subTest(name=name):
                study = root / "benchmarks" / name
                python = yaml.safe_load((study / "python/environment.yaml").read_text())
                r = yaml.safe_load((study / "r/environment.yaml").read_text())
                for definition in python["functions"].values():
                    definition.pop("function")
                for definition in r["functions"].values():
                    definition.pop("function")
                self.assertEqual(python["functions"], r["functions"])
                self.assertNotIn("contract_version", (study / "spec.yaml").read_text())


class TestRuleMetadata(unittest.TestCase):
    def test_requires_normative_rule_and_index_status(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            rules = root / "rules"
            rules.mkdir(parents=True)
            (rules / "README.md").write_text(
                "| ID | Rule | Status | Owns | Depends on |\n"
                "|---|---|---|---|---|\n"
                "| R001 | Rule | proposed | Topic | -- |\n"
            )
            (rules / "R001-rule.md").write_text(
                "---\nid: R001\ntitle: Rule\nstatus: proposed\n---\n"
            )

            errors = OWNER_EDITORIAL.validate_rule_metadata(root)

        self.assertEqual(len(errors), 2)
        self.assertTrue(all("normative" in error for error in errors))


class TestStableRequirementReferences(unittest.TestCase):
    def setUp(self):
        directory = self.enterContext(tempfile.TemporaryDirectory())
        self.root = Path(directory)
        self.enterContext(patch.object(RULE_CHECK, "REPO", self.root))
        self.rule = self.root / "R001-rule.md"
        self.error = self.root / "error.yaml"

    def check_rule(self, requirements, ending=None):
        self.rule.write_text(
            "---\nid: R001\ntitle: Rule\nstatus: normative\n"
            "applies_to: [root.rows]\n---\n"
            "# Rule\n\n## Intent\n\nIntent.\n\n"
            "## Boundaries\n\nBoundary.\n\n## Requirements\n\n"
            + requirements
            + "\n\n"
            + (ending or "## Errors\n\nErrors.\n\n## Rationale\n\nReason.\n")
        )
        errors, found = [], set()
        RULE_CHECK.check_rule(self.rule, errors, found)
        return errors, found

    def test_reordering_preserves_integer_and_suffix_citations(self):
        errors, found = self.check_rule(
            "**R001-12b.** Existing extension.\n\n"
            "**R001-2.** Moved topic.\n\n"
            "**R001-12a.** Existing extension.\n\n"
            "**R001-1.** Original topic."
        )
        for requirement in ("R001-1", "R001-2", "R001-12a", "R001-12b"):
            self.error.write_text(f"requirement: {requirement}\n")
            RULE_CHECK.check_error(self.error, found, errors)
        self.assertEqual(errors, [])
        self.assertEqual(found, {"R001-1", "R001-2", "R001-12a", "R001-12b"})

    def test_duplicate_requirement_ids_fail_including_suffixes(self):
        for identifier in ("R001-1", "R001-12a"):
            with self.subTest(identifier=identifier):
                errors, _ = self.check_rule(
                    f"**{identifier}.** First.\n\n**{identifier}.** Second."
                )
                self.assertTrue(any("duplicate requirements" in e for e in errors))

    def test_foreign_requirement_cannot_satisfy_a_citation(self):
        errors, found = self.check_rule("**R002-1.** Wrong owner.")
        self.assertTrue(any("requirements of other rules" in e for e in errors))
        self.error.write_text("requirement: R002-1\n")
        citation_errors = []
        RULE_CHECK.check_error(self.error, found, citation_errors)
        self.assertTrue(
            any("names no numbered requirement" in e for e in citation_errors)
        )

    def test_unresolved_and_malformed_citations_fail(self):
        _, found = self.check_rule("**R001-1.** Existing requirement.")
        for identifier in ("R001-2", "R001-1a", "R001-0", "R001-1ab"):
            with self.subTest(identifier=identifier):
                self.error.write_text(f"requirement: {identifier}\n")
                errors = []
                RULE_CHECK.check_error(self.error, found, errors)
                self.assertEqual(len(errors), 1)

    def test_numbered_forward_reference_keeps_old_citation_valid(self):
        errors, found = self.check_rule(
            "**R001-1.** See R011-35 for the canonical contract."
        )
        self.error.write_text("requirement: R001-1\n")
        RULE_CHECK.check_error(self.error, found, errors)
        self.assertEqual(errors, [])
        self.assertEqual(found, {"R001-1"})

    def test_topics_cannot_follow_errors_and_rationale(self):
        errors, _ = self.check_rule(
            "**R001-1.** Existing requirement.",
            "## Errors\n\nErrors.\n\n## Rationale\n\nReason.\n\n"
            "## Late topic\n\n**R001-2.** Appended topic.\n",
        )
        self.assertTrue(any("end with Errors and Rationale" in e for e in errors))

    def test_rule_without_requirement_ids_fails(self):
        errors, _ = self.check_rule("Unnumbered contract.")
        self.assertTrue(any("no numbered requirements" in e for e in errors))


class TestSpecificationInheritance(unittest.TestCase):
    def setUp(self):
        self.env, schema_errors = OWNER_REPOSITORY_SCHEMA.build_schema_env(ROOT)
        self.assertEqual(schema_errors, [])

    def resolve(self, path):
        with open(path, "r", encoding="utf-8") as handle:
            spec = yaml.load(handle, Loader=OWNER_REPOSITORY_YAML.UniqueKeyLoader)
        return OWNER_REPOSITORY_SCHEMA.resolve_spec_inheritance(
            spec, "example/spec.yaml", path, self.env
        )

    def test_bare_string_derivation_normalizes_to_source(self):
        # REQ-0319: the repository validator mirrors the engine's parse-time
        # expansion so resolved fixtures keep the canonical form.
        normalized = OWNER_REPOSITORY_SCHEMA.normalize_single_type_value(
            "DM.AGE", "derivation", self.env, False
        )
        self.assertEqual(normalized, {"value": {"source": {"variable": "DM.AGE"}}})

    def test_bare_case_result_normalizes_to_source(self):
        normalized = OWNER_REPOSITORY_SCHEMA.normalize_single_type_value(
            "DM.AGE", "case_result", self.env, False
        )
        self.assertEqual(normalized, {"source": {"variable": "DM.AGE"}})

    def test_resolves_shallow_diamond_and_minimal_ordered_spec(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            layers = root / "layers"
            inputs = root / "input"
            layers.mkdir()
            inputs.mkdir()
            (inputs / "dm.csv").write_text("USUBJID,AGE\n01,40\n")
            (layers / "common.yaml").write_text(
                'schema_version: "1.0"\n'
                "input:\n"
                "  DM: ../input/dm.csv\n"
                "  UNUSED: ../input/missing.csv\n"
                "base: DM\n"
                "intermediates:\n"
                "  - id: unused_lookup\n"
                "    dataset: UNUSED\n"
                "columns:\n"
                "  - name: RESULT\n"
                "    type: str\n"
                "    label: Common label\n"
                "    metadata: {owner: common}\n"
                "    derivation: {literal: common}\n"
                "  - name: DEPENDENT\n"
                "    type: str\n"
                "    label: Dependent\n"
                "    derivation: {source: LATE}\n"
                "  - name: UNUSED\n"
                "    type: str\n"
                "    label: Unused\n"
                "    derivation: {source: MISSING}\n"
                "  - name: AUDIT\n"
                "    type: str\n"
                "    label: Verification input\n"
                "    derivation: {literal: audit}\n"
                "verifications:\n"
                "  unique: {columns: [AUDIT]}\n"
            )
            (layers / "a.yaml").write_text(
                'schema_version: "1.0"\n'
                "parents: common.yaml\n"
                "metadata: {owner: a}\n"
                "columns:\n"
                "  - name: RESULT\n"
                "    label: Parent A label\n"
            )
            (layers / "b.yaml").write_text(
                'schema_version: "1.0"\n'
                "parents: common.yaml\n"
                "metadata: {owner: b}\n"
                "input:\n"
                "  DM:\n"
                "    types: {AGE: int}\n"
                "columns:\n"
                "  - name: RESULT\n"
                "    derivation: {literal: parent-b}\n"
                "    metadata: null\n"
                "  - name: LATE\n"
                "    type: str\n"
                "    label: Late dependency\n"
                "    derivation: {literal: ready}\n"
            )
            spec_path = root / "spec.yaml"
            spec_path.write_text(
                'schema_version: "1.0"\n'
                "parents: [layers/a.yaml, layers/b.yaml]\n"
                "domain: ADSL\n"
                "keys: [RESULT]\n"
                "output: {path: out.csv, columns: [RESULT, DEPENDENT]}\n"
            )

            resolved, errors, _ = self.resolve(spec_path)

        self.assertEqual(errors, [])
        self.assertNotIn("parents", resolved)
        self.assertEqual(resolved["metadata"], {"owner": "b"})
        self.assertEqual(
            resolved["input"],
            {
                "DM": {
                    "path": "input/dm.csv",
                    "types": {"AGE": "int"},
                    "empty_string": "missing",
                }
            },
        )
        self.assertNotIn("intermediates", resolved)
        self.assertEqual(
            [column["name"] for column in resolved["columns"]],
            ["RESULT", "AUDIT", "LATE", "DEPENDENT"],
        )
        result = resolved["columns"][0]
        self.assertEqual(result["label"], "Parent A label")
        self.assertNotIn("metadata", result)
        self.assertEqual(result["derivation"], {"value": {"literal": "parent-b"}})

    def test_merges_each_keyed_collection_at_the_member_boundary(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            parent = root / "parent.yaml"
            parent.write_text(
                'schema_version: "1.0"\n'
                "input:\n"
                "  DM: input/dm.csv\n"
                "  REF: input/ref.csv\n"
                "base: DM\n"
                "intermediates:\n"
                "  - id: ref\n"
                "    dataset: REF\n"
                "    key: {KEY: DM.KEY}\n"
                "columns:\n"
                "  - name: X\n"
                "    type: str\n"
                "    label: Value\n"
                "    derivation: {literal: root}\n"
                "  - {name: R, type: str, label: Reference, derivation: ref.VALUE}\n"
                "rows:\n"
                "  - id: main\n"
                "    dataset: DM\n"
                "    derivations:\n"
                "      X: {literal: parent}\n"
                "      UNUSED: {literal: parent}\n"
            )
            spec_path = root / "spec.yaml"
            spec_path.write_text(
                'schema_version: "1.0"\n'
                "parents: parent.yaml\n"
                "domain: TEST\n"
                "keys: [X]\n"
                "output: {path: out.csv, columns: [X, R]}\n"
                "intermediates:\n"
                "  - id: ref\n"
                "    no_match: 0\n"
                "rows:\n"
                "  - id: main\n"
                "    derivations:\n"
                "      X: {literal: child}\n"
            )

            resolved, errors, _ = self.resolve(spec_path)

        self.assertEqual(errors, [])
        self.assertEqual(
            resolved["intermediates"][0],
            {
                "id": "ref",
                "dataset": "REF",
                "key": {"KEY": "DM.KEY"},
                "no_match": 0,
            },
        )
        self.assertEqual(
            resolved["rows"][0],
            {
                "id": "main",
                "dataset": "DM",
                "derivations": {
                    "X": {"value": {"literal": "child"}},
                },
            },
        )

    def test_final_error_names_the_contributing_parent_file(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            parent = root / "parent.yaml"
            parent.write_text(
                'schema_version: "1.0"\n'
                "input: {DM: input/dm.csv}\n"
                "base: DM\n"
                "columns:\n"
                "  - name: A\n"
                "    label: Incomplete parent column\n"
            )
            spec_path = root / "spec.yaml"
            spec_path.write_text(
                'schema_version: "1.0"\n'
                "parents: parent.yaml\n"
                "domain: TEST\n"
                "keys: [A]\n"
                "output: {path: out.csv, columns: [A]}\n"
            )
            with open(spec_path, "r", encoding="utf-8") as handle:
                spec = yaml.load(handle, Loader=OWNER_REPOSITORY_YAML.UniqueKeyLoader)

            errors = OWNER_REPOSITORY_SCHEMA.validate_spec_document(
                spec, "example/spec.yaml", spec_path, self.env
            )

        self.assertTrue(errors)
        self.assertIn(f"(contributed by {parent.resolve()})", "\n".join(errors))

    def test_output_order_by_keeps_an_internal_column_live(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            layers = root / "layers"
            inputs = root / "input"
            layers.mkdir()
            inputs.mkdir()
            (inputs / "dm.csv").write_text("USUBJID,SITEORD\n01,2\n")
            (layers / "common.yaml").write_text(
                'schema_version: "1.0"\n'
                "input:\n"
                "  DM: ../input/dm.csv\n"
                "base: DM\n"
                "columns:\n"
                "  - name: USUBJID\n"
                "    type: str\n"
                "    label: Unique Subject Identifier\n"
                "    derivation: {source: DM.USUBJID}\n"
                "  - name: SITEORD\n"
                "    type: str\n"
                "    label: Site Sort Order\n"
                "    derivation: {source: DM.SITEORD}\n"
                "  - name: UNUSED\n"
                "    type: str\n"
                "    label: Unused\n"
                "    derivation: {literal: idle}\n"
            )
            spec_path = root / "spec.yaml"
            spec_path.write_text(
                'schema_version: "1.0"\n'
                "parents: layers/common.yaml\n"
                "domain: ADSL\n"
                "keys: [USUBJID]\n"
                "output:\n"
                "  path: out.csv\n"
                "  columns: [USUBJID]\n"
                "  order_by:\n"
                "    - {variable: SITEORD, direction: desc}\n"
            )

            resolved, errors, _ = self.resolve(spec_path)

        self.assertEqual(errors, [])
        self.assertEqual(
            [column["name"] for column in resolved["columns"]],
            ["USUBJID", "SITEORD"],
        )

    def test_resolved_column_requires_a_label(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            parent = root / "parent.yaml"
            parent.write_text(
                'schema_version: "1.0"\n'
                "input: {DM: input/dm.csv}\n"
                "base: DM\n"
                "columns:\n"
                "  - name: A\n"
                "    type: str\n"
                "    derivation: {literal: value}\n"
            )
            spec_path = root / "spec.yaml"
            spec_path.write_text(
                'schema_version: "1.0"\n'
                "parents: parent.yaml\n"
                "domain: TEST\n"
                "keys: [A]\n"
                "output: {path: out.csv, columns: [A]}\n"
            )
            with open(spec_path, "r", encoding="utf-8") as handle:
                spec = yaml.load(handle, Loader=OWNER_REPOSITORY_YAML.UniqueKeyLoader)

            errors = OWNER_REPOSITORY_SCHEMA.validate_spec_document(
                spec, "example/spec.yaml", spec_path, self.env
            )

        self.assertIn("requires a non-empty label", "\n".join(errors))

    def test_accepts_an_absolute_parent_path(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            parent = root / "parent.yaml"
            parent.write_text(
                'schema_version: "1.0"\ndomain: TEST\nkeys: [ID]\ninput: {DM: source.csv}\ncolumns: [{name: ID, type: str, derivation: DM.ID}]\n'
            )
            spec_path = root / "spec.yaml"
            spec_path.write_text(
                'schema_version: "1.0"\n'
                f'parents: "{parent.resolve()}"\n'
                "output: {path: out.csv, columns: [ID]}\n"
            )

            resolved, errors, _ = self.resolve(spec_path)

        self.assertEqual(errors, [])
        self.assertNotIn("parents", resolved)
        self.assertEqual(resolved["output"], {"path": "out.csv", "columns": ["ID"]})

    def test_entry_inherits_output_where_its_layer_names(self):
        # Issue #845: an entry may omit output (REQ-0623); the inherited
        # paths keep the provenance of the layer that wrote them (REQ-0629).
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            (root / "common").mkdir()
            (root / "study").mkdir()
            (root / "common" / "parent.yaml").write_text(
                'schema_version: "1.0"\n'
                "domain: TEST\nkeys: [ID]\ninput: {DM: source.csv}\ncolumns: [{name: ID, type: str, derivation: DM.ID}]\n"
                "output:\n"
                "  path: out.csv\n"
                "  columns: [ID]\n"
                "  warning_log: warnings.csv\n"
            )
            spec_path = root / "study" / "spec.yaml"
            spec_path.write_text(
                'schema_version: "1.0"\nparents: ../common/parent.yaml\n'
            )

            resolved, errors, _ = self.resolve(spec_path)

        self.assertEqual(errors, [])
        self.assertEqual(
            resolved["output"],
            {
                "path": "../common/out.csv",
                "columns": ["ID"],
                "warning_log": "../common/warnings.csv",
            },
        )

    def test_rejects_a_chain_no_layer_gives_output(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            parent = root / "parent.yaml"
            parent.write_text(
                'schema_version: "1.0"\n'
                "input: {DM: input/dm.csv}\n"
                "base: DM\n"
                "columns:\n"
                "  - name: A\n"
                "    type: str\n"
                "    label: A\n"
                "    derivation: {literal: value}\n"
            )
            spec_path = root / "spec.yaml"
            spec_path.write_text(
                'schema_version: "1.0"\nparents: parent.yaml\ndomain: TEST\nkeys: [A]\n'
            )
            with open(spec_path, "r", encoding="utf-8") as handle:
                spec = yaml.load(handle, Loader=OWNER_REPOSITORY_YAML.UniqueKeyLoader)

            errors = OWNER_REPOSITORY_SCHEMA.validate_spec_document(
                spec, "example/spec.yaml", spec_path, self.env
            )

        # REQ-0657: requiredness applies to the resolved specification.
        self.assertIn("missing_required_field", "\n".join(errors))
        self.assertIn("example/spec.yaml.output", "\n".join(errors))

    def test_rejects_parent_url(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            spec_path = Path(temp_dir) / "spec.yaml"
            spec_path.write_text(
                'schema_version: "1.0"\n'
                "parents: https://example.test/base.yaml\n"
                "output: {path: out.csv, columns: []}\n"
            )

            resolved, errors, _ = self.resolve(spec_path)

        self.assertIsNone(resolved)
        self.assertIn("invalid_parent_path", "\n".join(errors))

    def test_rejects_missing_parent(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            spec_path = Path(temp_dir) / "spec.yaml"
            spec_path.write_text(
                'schema_version: "1.0"\n'
                "parents: missing.yaml\n"
                "output: {path: out.csv, columns: []}\n"
            )

            resolved, errors, _ = self.resolve(spec_path)

        self.assertIsNone(resolved)
        self.assertIn("parent_not_found", "\n".join(errors))

    def test_rejects_inheritance_cycle(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            spec_path = root / "spec.yaml"
            parent = root / "parent.yaml"
            spec_path.write_text(
                'schema_version: "1.0"\n'
                "parents: parent.yaml\n"
                "output: {path: out.csv, columns: []}\n"
            )
            parent.write_text('schema_version: "1.0"\nparents: spec.yaml\n')

            resolved, errors, _ = self.resolve(spec_path)

        self.assertIsNone(resolved)
        self.assertIn("inheritance_cycle", "\n".join(errors))

    def test_inherited_input_absent_beside_its_layer_never_retries_the_entry(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            (root / "common").mkdir()
            (root / "input").mkdir()
            (root / "input" / "dm.csv").write_text("ID\n01\n")
            (root / "common" / "base.yaml").write_text(
                'schema_version: "1.0"\n'
                "domain: TEST\n"
                "keys: [ID]\n"
                "input: {DM: input/dm.csv}\n"
                "base: DM\n"
                "columns:\n"
                "  - {name: ID, type: str, label: Identifier, "
                "derivation: DM.ID}\n"
            )
            spec_path = root / "spec.yaml"
            spec_path.write_text(
                'schema_version: "1.0"\n'
                "parents: common/base.yaml\n"
                "output: {path: out.csv, columns: [ID]}\n"
            )
            with open(spec_path, "r", encoding="utf-8") as handle:
                spec = yaml.load(handle, Loader=OWNER_REPOSITORY_YAML.UniqueKeyLoader)

            resolved, _, _ = self.resolve(spec_path)
            errors = OWNER_REPOSITORY_SCHEMA.validate_spec_document(
                spec, "example/spec.yaml", spec_path, self.env
            )
            (root / "input" / "dm.csv").unlink()
            missing = OWNER_REPOSITORY_SCHEMA.validate_spec_document(
                spec, "example/spec.yaml", spec_path, self.env
            )

        # REQ-0636 rebases the declaration; REQ-1246 forbids entry fallback.
        self.assertEqual(resolved["input"]["DM"]["path"], "common/input/dm.csv")
        self.assertIn("resource_path_missing", "\n".join(errors))
        self.assertIn(
            "input.DM.path: resource_path_missing: 'input/dm.csv'",
            "\n".join(missing),
        )

    def test_rejects_layer_version_mismatch(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            spec_path = root / "spec.yaml"
            parent = root / "parent.yaml"
            spec_path.write_text(
                'schema_version: "1.0"\n'
                "parents: parent.yaml\n"
                "output: {path: out.csv, columns: []}\n"
            )
            parent.write_text('schema_version: "2.0"\n')

            resolved, errors, _ = self.resolve(spec_path)

        self.assertIsNone(resolved)
        self.assertIn("schema_version_mismatch", "\n".join(errors))

    def test_rejects_clearing_required_or_absent_fields(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            parent = root / "parent.yaml"
            parent.write_text(
                'schema_version: "1.0"\n'
                "columns:\n"
                "  - name: A\n"
                "    type: str\n"
                "    derivation: {literal: value}\n"
            )
            spec_path = root / "spec.yaml"
            spec_path.write_text(
                'schema_version: "1.0"\n'
                "parents: parent.yaml\n"
                "output: {path: out.csv, columns: [A]}\n"
                "columns:\n"
                "  - name: A\n"
                "    type: null\n"
                "    label: null\n"
            )

            resolved, errors, _ = self.resolve(spec_path)

        self.assertIsNone(resolved)
        message = "\n".join(errors)
        self.assertIn("invalid_clear", message)
        self.assertIn("columns.A.type", message)

    def test_rejects_duplicate_member_within_one_layer(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            spec_path = Path(temp_dir) / "spec.yaml"
            spec_path.write_text(
                'schema_version: "1.0"\n'
                "parents: []\n"
                "output: {path: out.csv, columns: []}\n"
                "columns:\n"
                "  - {name: A}\n"
                "  - {name: A}\n"
            )

            resolved, errors, _ = self.resolve(spec_path)

        self.assertIsNone(resolved)
        self.assertIn("duplicate_identifier", "\n".join(errors))


class TestTypeValidation(unittest.TestCase):
    def test_int_rejects_string(self):
        errors = OWNER_REPOSITORY_SCHEMA.validate_type(
            "not-an-int",
            ["int"],
            {"classes": {}, "aliases": {}, "registries": {}},
            "spec.count",
        )
        self.assertTrue(errors)
        self.assertIn("expected int", errors[0])

    def test_registry_value_requires_exactly_one_operation(self):
        env = {
            "classes": {},
            "aliases": {"operation": {"registry": "operations"}},
            "registries": {
                "operations": {
                    "first": {"type": "str"},
                    "second": {"type": "str"},
                }
            },
        }
        errors = OWNER_REPOSITORY_SCHEMA.validate_type(
            {"first": "a", "second": "b"},
            ["operation"],
            env,
            "spec.operation",
        )
        self.assertTrue(errors)
        self.assertIn("exactly one", errors[0])

    def test_alias_values_constraint(self):
        env = {
            "classes": {},
            "aliases": {"status": {"type": "str", "values": ["A", "B"]}},
            "registries": {},
        }
        errors = OWNER_REPOSITORY_SCHEMA.validate_type(
            "C", ["status"], env, "spec.status"
        )
        self.assertTrue(errors)
        self.assertIn("allowed values", errors[0])

    def test_alias_pattern_constraint(self):
        env = {
            "classes": {},
            "aliases": {"variable": {"type": "str", "pattern": "^[A-Z]+$"}},
            "registries": {},
        }
        errors = OWNER_REPOSITORY_SCHEMA.validate_type(
            "lower", ["variable"], env, "spec.name"
        )
        self.assertTrue(errors)
        self.assertIn("pattern", errors[0])

    def test_alias_min_length_requires_string(self):
        env = {
            "classes": {},
            "aliases": {"variables": {"type": "list[str]", "min_length": 1}},
            "registries": {},
        }
        errors = OWNER_REPOSITORY_SCHEMA.check_descriptor(
            env["aliases"]["variables"], False, "schema.variables"
        )
        self.assertTrue(errors)
        self.assertIn("minimum length requires a string", errors[0])

    def test_alias_size_constraint(self):
        env = {
            "classes": {},
            "aliases": {"pair": {"type": "list[str]", "size": 2}},
            "registries": {},
        }
        errors = OWNER_REPOSITORY_SCHEMA.validate_type(
            ["one"], ["pair"], env, "spec.pair"
        )
        self.assertTrue(errors)
        self.assertIn("size 2", errors[0])

    def test_descriptor_rejects_constraint_on_wrong_type(self):
        errors = OWNER_REPOSITORY_SCHEMA.check_descriptor(
            {"type": "int", "pattern": "^[0-9]+$"},
            False,
            "schema:count",
        )
        self.assertTrue(errors)
        self.assertIn("pattern", "\n".join(errors))

    def test_descriptor_rejects_default_on_required_field(self):
        errors = OWNER_REPOSITORY_SCHEMA.check_descriptor(
            {"type": "str", "required": True, "default": "x"},
            True,
            "schema:name",
        )
        self.assertTrue(errors)
        self.assertIn("default", "\n".join(errors))

    def test_nested_dict_type_expression(self):
        env = {"classes": {}, "aliases": {}, "registries": {}}
        valid = {"outer": {"inner": 1}}
        self.assertEqual(
            OWNER_REPOSITORY_SCHEMA.validate_type(
                valid,
                ["dict[str, dict[str, int]]"],
                env,
                "spec.mapping",
            ),
            [],
        )
        errors = OWNER_REPOSITORY_SCHEMA.validate_type(
            {"outer": {"inner": "wrong"}},
            ["dict[str, dict[str, int]]"],
            env,
            "spec.mapping",
        )
        self.assertTrue(errors)
        self.assertIn("expected int", errors[0])

    def test_bare_list_and_dict_types(self):
        env = {"classes": {}, "aliases": {}, "registries": {}}
        self.assertEqual(
            OWNER_REPOSITORY_SCHEMA.validate_type([], ["list"], env, "spec.items"), []
        )
        self.assertEqual(
            OWNER_REPOSITORY_SCHEMA.validate_type({}, ["dict"], env, "spec.mapping"), []
        )

    def test_size_accepts_bare_list_and_dict_types(self):
        for type_name in ("list", "dict"):
            with self.subTest(type_name=type_name):
                self.assertEqual(
                    OWNER_REPOSITORY_SCHEMA.check_descriptor(
                        {"type": type_name, "size": 0},
                        is_class_field=False,
                        path="schema.value",
                    ),
                    [],
                )

    def test_union_prefers_a_matching_outer_type_constraint_error(self):
        env = {
            "classes": {},
            "aliases": {
                "day_rule": {
                    "type": "str",
                    "values": ["first", "last"],
                }
            },
            "registries": {},
        }

        errors = OWNER_REPOSITORY_SCHEMA.validate_type(
            "middle", ["int", "day_rule"], env, "spec.day"
        )

        self.assertEqual(len(errors), 1)
        self.assertIsInstance(
            errors[0], OWNER_VALIDATION_DIAGNOSTICS.ValidationDiagnostic
        )
        self.assertEqual(errors[0].condition, "value_not_permitted")


class TestDateImputeSchema(unittest.TestCase):
    def setUp(self):
        self.env, schema_errors = OWNER_REPOSITORY_SCHEMA.build_schema_env(ROOT)
        self.assertEqual(schema_errors, [])

    def test_accepts_month_minimum_source_precision(self):
        errors = OWNER_REPOSITORY_SCHEMA.validate_type(
            {
                "date_impute": {
                    "source": "AE.AESTDTC",
                    "month": 6,
                    "day": 15,
                    "minimum_source_precision": "month",
                }
            },
            ["expression"],
            self.env,
            "spec.columns.ASTDT.derivation",
        )

        self.assertEqual(errors, [])

    def test_rejects_unknown_minimum_source_precision(self):
        errors = OWNER_REPOSITORY_SCHEMA.validate_type(
            {
                "date_impute": {
                    "source": "AE.AESTDTC",
                    "month": 6,
                    "day": 15,
                    "minimum_source_precision": "day",
                }
            },
            ["expression"],
            self.env,
            "spec.columns.ASTDT.derivation",
        )

        self.assertTrue(errors)
        self.assertIn("allowed values", errors[0])

    def test_accepts_integer_and_month_position_day(self):
        for day in (15, "first", "last"):
            with self.subTest(day=day):
                errors = OWNER_REPOSITORY_SCHEMA.validate_type(
                    {
                        "date_impute": {
                            "source": "AE.AESTDTC",
                            "month": 6,
                            "day": day,
                        }
                    },
                    ["expression"],
                    self.env,
                    "spec.columns.ASTDT.derivation",
                )

                self.assertEqual(errors, [])

    def test_rejects_unknown_day_position(self):
        errors = OWNER_REPOSITORY_SCHEMA.validate_type(
            {
                "date_impute": {
                    "source": "AE.AESTDTC",
                    "month": 6,
                    "day": "mid",
                }
            },
            ["expression"],
            self.env,
            "spec.columns.ASTDT.derivation",
        )

        self.assertTrue(errors)

    def test_accepts_not_before_bound(self):
        errors = OWNER_REPOSITORY_SCHEMA.validate_type(
            {
                "date_impute": {
                    "source": "AE.AESTDTC",
                    "month": 6,
                    "day": "last",
                    "not_before": "TRTSDT",
                }
            },
            ["expression"],
            self.env,
            "spec.columns.ASTDT.derivation",
        )

        self.assertEqual(errors, [])

    def test_rejects_unknown_date_impute_field(self):
        errors = OWNER_REPOSITORY_SCHEMA.validate_type(
            {
                "date_impute": {
                    "source": "AE.AESTDTC",
                    "month": 6,
                    "day": 15,
                    "not_after": "TRTSDT",
                }
            },
            ["expression"],
            self.env,
            "spec.columns.ASTDT.derivation",
        )

        self.assertTrue(errors)


class TestToDateSchema(unittest.TestCase):
    def setUp(self):
        self.env, schema_errors = OWNER_REPOSITORY_SCHEMA.build_schema_env(ROOT)
        self.assertEqual(schema_errors, [])

    def validate(self, payload):
        return OWNER_REPOSITORY_SCHEMA.validate_type(
            {"to_date": payload},
            ["expression"],
            self.env,
            "spec.columns.ASTDT2.derivation",
        )

    def test_accepts_source_variable_shape(self):
        self.assertEqual(self.validate({"source": "ASTDTM"}), [])

    def test_rejects_missing_source_and_unknown_fields(self):
        missing_source = self.validate({})
        unknown_field = self.validate({"source": "ASTDTM", "timezone": "UTC"})

        self.assertIn("missing required field 'source'", missing_source[0])
        self.assertIn("unknown field 'timezone'", unknown_field[0])


class TestNamedWindows(unittest.TestCase):
    def setUp(self):
        self.env, errors = OWNER_REPOSITORY_SCHEMA.build_schema_env(ROOT)
        self.assertEqual(errors, [])
        self.path = ROOT / "benchmarks/adam-adrs-confirmed-response/spec.yaml"
        self.spec = yaml.load(
            self.path.read_text(), Loader=OWNER_REPOSITORY_YAML.UniqueKeyLoader
        )

    def prepare(self, spec):
        return OWNER_REPOSITORY_SCHEMA.prepare_spec_document(
            spec, "spec", self.path, self.env
        )

    def test_expands_full_references_without_mutating_authoring(self):
        resolved, errors, _ = self.prepare(self.spec)
        self.assertEqual(errors, [])
        self.assertNotIn("windows", resolved)
        windows = [
            column["derivation"]["value"]["row_value"]["window"]
            for column in resolved["columns"]
            if isinstance(column.get("derivation"), dict)
            and "row_value" in column["derivation"].get("value", {})
        ]
        self.assertEqual(len(windows), 2)
        self.assertEqual(
            windows[0],
            {
                "group_by": ["STUDYID", "USUBJID"],
                "order_by": [
                    {"variable": "ADT", "direction": "asc", "nulls": "last"},
                    {"variable": "RSSEQ", "direction": "asc", "nulls": "last"},
                ],
            },
        )
        self.assertEqual(windows[0], windows[1])
        self.assertIsNot(windows[0], windows[1])
        self.assertIn("windows", self.spec)

    def test_unknown_reference_keeps_structured_diagnostic(self):
        self.spec.pop("windows")
        _, errors, _ = self.prepare(self.spec)
        self.assertEqual(len(errors), 2)
        self.assertEqual(errors[0].condition, "unknown_window")
        self.assertEqual(errors[0].context, {"window": "RESPONSE_ORDER"})
        self.assertTrue(errors[0].path.endswith(".row_value.window"))
        self.assertTrue(errors[0].path.startswith("spec.columns."))

    def test_unused_malformed_definition_is_not_discarded(self):
        for definition in ("RESPONSE_ORDER", {"ref": "RESPONSE_ORDER"}, {"unknown": 1}):
            with self.subTest(definition=definition):
                self.spec["windows"]["UNUSED"] = definition
                _, errors, _ = self.prepare(self.spec)
                self.assertTrue(errors)

    def test_reference_mapping_is_rejected_by_schema(self):
        for value in (
            {"ref": "RESPONSE_ORDER"},
            {"ref": "RESPONSE_ORDER", "group_by": ["USUBJID"]},
        ):
            errors = OWNER_REPOSITORY_SCHEMA.validate_type(
                {"row_value": {"source": "ADT", "offset": 1, "window": value}},
                ["expression"],
                self.env,
                "spec.columns.NEXTDT.derivation",
            )
            self.assertTrue(errors)

    def test_inheritance_replaces_definitions_before_dependency_pruning(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            parent = root / "parent.yaml"
            parent.write_text(yaml.safe_dump(self.spec, sort_keys=False))
            child = {
                "schema_version": "1.0",
                "parents": "parent.yaml",
                "windows": {
                    "RESPONSE_ORDER": {"order_by": ["ADT"]},
                    "UNUSED": {"group_by": ["ABSENT"]},
                },
            }
            resolved, errors, _ = OWNER_REPOSITORY_SCHEMA.resolve_spec_inheritance(
                child, "spec", root / "spec.yaml", self.env
            )
            self.assertEqual(errors, [])
            nextdt = next(c for c in resolved["columns"] if c["name"] == "NEXTDT")
            window = nextdt["derivation"]["value"]["row_value"]["window"]
            self.assertNotIn("group_by", window)
            self.assertEqual(window["order_by"][0]["variable"], "ADT")
            self.assertNotIn("windows", resolved)


class TestPreviousNonMissingSchema(unittest.TestCase):
    def setUp(self):
        self.env, schema_errors = OWNER_REPOSITORY_SCHEMA.build_schema_env(ROOT)
        self.assertEqual(schema_errors, [])

    def validate(self, payload):
        return OWNER_REPOSITORY_SCHEMA.validate_type(
            {"previous_non_missing": payload},
            ["expression"],
            self.env,
            "spec.columns.PREVIOUS.derivation",
        )

    def test_accepts_closed_window_inputs(self):
        errors = self.validate(
            {
                "source": "AVAL",
                "window": {
                    "group_by": ["STUDYID", "USUBJID", "PARAMCD"],
                    "order_by": [
                        {
                            "variable": "ADT",
                            "direction": "asc",
                            "nulls": "last",
                        }
                    ],
                },
            }
        )

        self.assertEqual(errors, [])

    def test_allows_one_partition_by_omission(self):
        self.assertEqual(
            self.validate({"source": "AVAL", "window": {"order_by": ["ADT"]}}),
            [],
        )

    def test_rejects_missing_source(self):
        missing_source = self.validate({"window": {"order_by": ["ADT"]}})

        self.assertIn("missing required field 'source'", missing_source[0])

    def test_accepts_window_filter(self):
        # The shared window_spec offers filter to every window expression;
        # the engine already honored it, only the schema withheld it.
        filtered = self.validate(
            {
                "source": "AVAL",
                "window": {
                    "order_by": ["ADT"],
                    "filter": "PARAMCD = 'WEIGHT'",
                },
            }
        )

        self.assertEqual(filtered, [])


class TestGroupedRows(unittest.TestCase):
    def test_accepts_driver_qualified_group_variables(self):
        spec = {
            "base": "ADLBIN",
            "rows": [
                {
                    "id": "derived",
                    "group_by": ["ADLBIN.USUBJID", "ADLBIN.VISIT"],
                }
            ],
        }

        errors = CONTROLS.grouped_rows(spec, "example/spec.yaml")

        self.assertEqual(errors, [])

    def test_rejects_empty_group(self):
        spec = {
            "base": "ADLBIN",
            "rows": [{"id": "derived", "group_by": []}],
        }

        errors = CONTROLS.grouped_rows(spec, "example/spec.yaml")

        self.assertTrue(errors)
        self.assertEqual(errors[0].context, {"group_by": [], "row": "derived"})

    def test_rejects_unqualified_or_wrong_driver_variable(self):
        spec = {
            "base": "ADLBIN",
            "rows": [
                {
                    "id": "derived",
                    "dataset": "ADLBIN",
                    "group_by": ["USUBJID", "OTHER.VISIT"],
                }
            ],
        }

        errors = CONTROLS.grouped_rows(spec, "example/spec.yaml")

        self.assertEqual(len(errors), 2)
        self.assertEqual(
            [error.context["dataset"] for error in errors], ["ADLBIN", "ADLBIN"]
        )

    def test_rejects_duplicate_group_variable(self):
        spec = {
            "base": "ADLBIN",
            "rows": [
                {
                    "id": "derived",
                    "group_by": ["ADLBIN.USUBJID", "ADLBIN.USUBJID"],
                }
            ],
        }

        errors = CONTROLS.grouped_rows(spec, "example/spec.yaml")

        self.assertTrue(errors)
        self.assertEqual(errors[0].condition, "invalid_field_type")
        self.assertEqual(
            errors[0].context["group_by"], ["ADLBIN.USUBJID", "ADLBIN.USUBJID"]
        )


class TestSpecNames(unittest.TestCase):
    def test_accepts_resolved_unique_names(self):
        spec = {
            "domain": "ADSL",
            "input": {"DM": "dm.csv", "EX": "ex.csv"},
            "base": "DM",
            "intermediates": [{"id": "dose", "dataset": "EX"}],
            "keys": ["USUBJID"],
            "output": {"columns": ["USUBJID"]},
            "columns": [{"name": "USUBJID"}],
            "rows": [{"id": "subjects", "dataset": "DM"}],
        }

        self.assertEqual(CONTROLS.names(spec, "example/spec.yaml"), [])

    def test_rejects_a_colliding_or_unknown_warning_log_path(self):
        base = {
            "domain": "ADSL",
            "input": {"DM": "dm.csv"},
            "base": "DM",
            "keys": ["USUBJID"],
            "columns": [{"name": "USUBJID"}],
        }
        colliding = copy.deepcopy(base)
        colliding["output"] = {
            "path": "adsl.csv",
            "columns": ["USUBJID"],
            "warning_log": "adsl.csv",
        }
        unknown = copy.deepcopy(base)
        unknown["output"] = {
            "path": "adsl.csv",
            "columns": ["USUBJID"],
            "warning_log": "adsl.txt",
        }

        collision_errors = CONTROLS.names(colliding, "example/spec.yaml")
        unknown_errors = CONTROLS.names(unknown, "example/spec.yaml")

        self.assertEqual(
            sum("artifact_path_collision" in item for item in collision_errors),
            2,
        )
        self.assertIn("unknown_artifact_profile", "\n".join(unknown_errors))

    def test_rejects_a_colliding_or_unknown_verification_log_path(self):
        base = {
            "domain": "ADSL",
            "input": {"DM": "dm.csv"},
            "base": "DM",
            "keys": ["USUBJID"],
            "columns": [{"name": "USUBJID"}],
        }
        primary_collision = copy.deepcopy(base)
        primary_collision["output"] = {
            "path": "adsl.csv",
            "columns": ["USUBJID"],
            "verification_log": "adsl.csv",
        }
        log_collision = copy.deepcopy(base)
        log_collision["output"] = {
            "path": "adsl.csv",
            "columns": ["USUBJID"],
            "warning_log": "adsl-checks.csv",
            "verification_log": "adsl-checks.csv",
        }
        unknown = copy.deepcopy(base)
        unknown["output"] = {
            "path": "adsl.csv",
            "columns": ["USUBJID"],
            "verification_log": "adsl-checks.txt",
        }
        governed = copy.deepcopy(base)
        governed["output"] = {
            "path": "adsl.csv",
            "columns": ["USUBJID"],
            "warning_log": "adsl-violations.csv",
            "verification_log": "adsl-checks.parquet",
        }

        primary_errors = CONTROLS.names(primary_collision, "example/spec.yaml")
        log_errors = CONTROLS.names(log_collision, "example/spec.yaml")
        unknown_errors = CONTROLS.names(unknown, "example/spec.yaml")
        governed_errors = CONTROLS.names(governed, "example/spec.yaml")

        self.assertEqual(
            sum("artifact_path_collision" in item for item in primary_errors),
            2,
        )
        self.assertEqual(
            sum("artifact_path_collision" in item for item in log_errors),
            2,
        )
        self.assertIn("unknown_artifact_profile", "\n".join(unknown_errors))
        self.assertEqual(governed_errors, [])

    def test_rejects_duplicate_and_unresolved_columns(self):
        spec = {
            "domain": "ADSL",
            "input": {"DM": "dm.csv"},
            "keys": ["MISSING", "MISSING"],
            "output": {"columns": ["USUBJID", "USUBJID"]},
            "columns": [{"name": "USUBJID"}, {"name": "USUBJID"}],
        }

        errors = CONTROLS.names(spec, "example/spec.yaml")

        self.assertIn("duplicate_identifier", [error.condition for error in errors])
        self.assertIn(
            "example/spec.yaml.columns[1].name", [error.path for error in errors]
        )
        self.assertIn("example/spec.yaml.keys[1]", [error.path for error in errors])
        self.assertIn("undeclared_column", [error.condition for error in errors])
        spec["columns"].pop()
        spec["keys"] = ["USUBJID"]
        projection = CONTROLS.names(spec, "example/spec.yaml")
        self.assertEqual(
            [(error.condition, error.path, error.context) for error in projection],
            [
                (
                    "duplicate_identifier",
                    "example/spec.yaml.output.columns[1]",
                    {"column": "USUBJID"},
                )
            ],
        )

    def test_rejects_empty_keys(self):
        spec = {
            "domain": "ADSL",
            "input": {"DM": "dm.csv"},
            "keys": [],
            "output": {"columns": ["USUBJID"]},
            "columns": [{"name": "USUBJID"}],
        }

        errors = CONTROLS.names(spec, "example/spec.yaml")

        self.assertIn("at least one key column", "\n".join(errors))

    def test_an_internal_key_names_only_its_key_entry(self):
        spec = {
            "domain": "ADSL",
            "input": {"DM": "dm.csv"},
            "keys": ["USUBJID", "SITEID"],
            "output": {"columns": ["USUBJID"]},
            "columns": [{"name": "USUBJID"}, {"name": "SITEID"}],
        }

        errors = CONTROLS.names(spec, "example/spec.yaml")

        diagnostics = [
            error
            for error in errors
            if isinstance(error, OWNER_VALIDATION_DIAGNOSTICS.ValidationDiagnostic)
            and error.condition == "internal_column_in_keys"
        ]
        self.assertEqual(
            [error.path for error in diagnostics],
            ["example/spec.yaml.keys[1]"],
        )

    def test_rejects_non_scalar_output_columns_without_crashing(self):
        for value, expected_type in (({}, "dict"), ([], "list")):
            with self.subTest(value=expected_type):
                spec = {
                    "domain": "ADSL",
                    "input": {"DM": "dm.csv"},
                    "keys": ["USUBJID"],
                    "output": {"columns": [value]},
                    "columns": [{"name": "USUBJID"}],
                }

                root = ROOT
                env, env_errors = OWNER_REPOSITORY_SCHEMA.build_schema_env(root)
                self.assertEqual(env_errors, [])
                errors = typed_schema_control(
                    spec,
                    "example/spec.yaml",
                    None,
                    {
                        "classes": {
                            "root_class": [{"output": {"type": "output_class"}}],
                            "output_class": [{"columns": {"type": "list[str]"}}],
                        }
                    },
                )

                diagnostics = [
                    error
                    for error in errors
                    if isinstance(
                        error, OWNER_VALIDATION_DIAGNOSTICS.ValidationDiagnostic
                    )
                    and error.path == "example/spec.yaml.output.columns[0]"
                ]
                self.assertEqual(len(diagnostics), 1)
                self.assertEqual(
                    diagnostics[0].path, "example/spec.yaml.output.columns[0]"
                )
                self.assertEqual(diagnostics[0].condition, "invalid_field_type")

    def test_accepts_output_order_by_over_declared_columns(self):
        spec = {
            "domain": "ADSL",
            "input": {"DM": "dm.csv"},
            "base": "DM",
            "keys": ["USUBJID"],
            "output": {
                "columns": ["USUBJID"],
                "order_by": [
                    "USUBJID",
                    {"variable": "SORTORD", "direction": "desc"},
                ],
            },
            "columns": [{"name": "USUBJID"}, {"name": "SORTORD"}],
        }

        self.assertEqual(CONTROLS.names(spec, "example/spec.yaml"), [])

    def test_rejects_unknown_and_repeated_output_order_terms(self):
        spec = {
            "domain": "ADSL",
            "input": {"DM": "dm.csv"},
            "base": "DM",
            "keys": ["USUBJID"],
            "output": {
                "columns": ["USUBJID"],
                "order_by": [
                    "USUBJID",
                    {"variable": "USUBJID", "nulls": "first"},
                    "DM.USUBJID",
                    "MISSING",
                ],
            },
            "columns": [{"name": "USUBJID"}],
        }

        errors = CONTROLS.names(spec, "example/spec.yaml")

        self.assertEqual(
            [(error.path, error.condition, error.context) for error in errors],
            [
                (
                    "example/spec.yaml.output.order_by[1]",
                    "duplicate_order_term",
                    {"column": "USUBJID"},
                ),
                (
                    "example/spec.yaml.output.order_by[2]",
                    "undeclared_column",
                    {"column": "DM.USUBJID"},
                ),
                (
                    "example/spec.yaml.output.order_by[3]",
                    "undeclared_column",
                    {"column": "MISSING"},
                ),
            ],
        )

    def test_rejects_dataset_and_lookup_namespace_errors(self):
        spec = {
            "domain": "ADSL",
            "input": {"ADSL": "input.csv"},
            "base": "MISSING",
            "intermediates": [
                {"id": "ADSL", "dataset": "MISSING"},
                {"id": "ADSL", "dataset": "ADSL"},
            ],
            "keys": [],
            "output": {"columns": []},
            "columns": [],
            "rows": [{"id": "row"}, {"id": "row", "dataset": "MISSING"}],
        }
        errors = CONTROLS.names(spec, "example/spec.yaml")
        facts = [(error.condition, error.path, error.context) for error in errors]
        for path in ("input.ADSL", "domain", "rows[1].id", "rows[0].id"):
            self.assertIn(
                (
                    "duplicate_identifier",
                    "example/spec.yaml." + path,
                    {"identifier": "row" if path.startswith("rows") else "ADSL"},
                ),
                facts,
            )
        self.assertIn(
            ("driver_unavailable", "example/spec.yaml.base", {"dataset": "MISSING"}),
            facts,
        )
        self.assertIn(
            (
                "driver_unavailable",
                "example/spec.yaml.rows[1].dataset",
                {"dataset": "MISSING", "row": "row"},
            ),
            facts,
        )

        # Earlier root failures cannot stand in for later lookup obligations.
        # Isolate each original assertion with complete, otherwise valid names.
        spec.update(domain="OUT", base="ADSL", keys=["ID"], columns=[{"name": "ID"}])
        spec["output"]["columns"] = ["ID"]
        spec.pop("rows")
        for declarations, expected in (
            (
                [{"id": "LOOK", "dataset": "MISSING"}],
                [
                    (
                        "unknown_field",
                        "intermediates[0].dataset",
                        {"identifier": "MISSING", "intermediate": "LOOK"},
                    )
                ],
            ),
            (
                [{"id": "LOOK", "dataset": "ADSL"}, {"id": "LOOK", "dataset": "ADSL"}],
                [
                    (
                        "duplicate_identifier",
                        "intermediates[1].id",
                        {"identifier": "LOOK"},
                    ),
                    (
                        "duplicate_identifier",
                        "intermediates[0].id",
                        {"identifier": "LOOK"},
                    ),
                ],
            ),
            (
                [{"id": "ADSL", "dataset": "ADSL"}],
                [
                    (
                        "duplicate_identifier",
                        "intermediates[0].id",
                        {"identifier": "ADSL"},
                    ),
                    ("duplicate_identifier", "input.ADSL", {"identifier": "ADSL"}),
                ],
            ),
            (
                [{"id": "OUT", "dataset": "ADSL"}],
                [
                    (
                        "duplicate_identifier",
                        "intermediates[0].id",
                        {"identifier": "OUT"},
                    ),
                    ("duplicate_identifier", "domain", {"identifier": "OUT"}),
                ],
            ),
        ):
            with self.subTest(declarations=declarations):
                spec["intermediates"] = declarations
                findings = CONTROLS.names(spec, "example/spec.yaml")
                self.assertEqual(
                    [
                        (
                            error.condition,
                            error.path.removeprefix("example/spec.yaml."),
                            error.context,
                        )
                        for error in findings
                    ],
                    expected,
                )

    def test_accepts_intermediate_row_driver(self):
        # REQ-1262: a row template may name a named intermediate as
        # its driver besides an input dataset.
        spec = {
            "domain": "ADAE",
            "input": {"AE": "input/ae.csv"},
            "intermediates": [
                {"id": "SERIOUS_AE", "dataset": "AE"},
            ],
            "keys": ["STUDYID"],
            "output": {"columns": ["STUDYID"]},
            "columns": [{"name": "STUDYID"}],
            "rows": [{"id": "serious", "dataset": "SERIOUS_AE"}],
        }

        errors = CONTROLS.names(spec, "example/spec.yaml")

        self.assertEqual(errors, [])


class TestSpecContracts(unittest.TestCase):
    def test_rejects_missing_base_and_incomplete_column_coverage(self):
        spec = {
            "domain": "ADSL",
            "input": {"DM": "dm.csv", "VS": "vs.csv"},
            "keys": ["USUBJID"],
            "output": {"columns": ["USUBJID", "AGE"]},
            "columns": [
                {"name": "USUBJID", "derivation": {"source": "DM.USUBJID"}},
                {"name": "AGE"},
            ],
        }

        errors = CONTROLS.contracts(spec, "example/spec.yaml")

        message = "\n".join(errors)
        self.assertIn("driver_unavailable", message)
        self.assertIn("AGE", message)
        self.assertIn("missing_derivation", message)

    def test_accepts_missing_base_with_one_dataset(self):
        spec = {
            "domain": "ADSL",
            "input": {"DM": "dm.csv"},
            "keys": ["USUBJID"],
            "output": {"columns": ["USUBJID"]},
            "columns": [
                {"name": "USUBJID", "derivation": {"source": "DM.USUBJID"}},
            ],
        }

        self.assertEqual(CONTROLS.contracts(spec, "example/spec.yaml"), [])

    def test_warning_verification_requires_a_warning_log(self):
        spec = {
            "domain": "ADSL",
            "input": {"DM": "dm.csv"},
            "base": "DM",
            "keys": ["USUBJID"],
            "output": {"path": "adsl.csv", "columns": ["USUBJID", "AGE"]},
            "columns": [
                {
                    "name": "USUBJID",
                    "type": "str",
                    "derivation": {"source": "DM.USUBJID"},
                },
                {
                    "name": "AGE",
                    "type": "int",
                    "derivation": {"source": "DM.AGE"},
                    "verifications": [{"range": {"min": 18, "severity": "warning"}}],
                },
            ],
        }

        missing = CONTROLS.contracts(spec, "example/spec.yaml")
        spec["output"]["warning_log"] = "adsl-violations.csv"
        governed = CONTROLS.contracts(spec, "example/spec.yaml")

        self.assertIn("missing_warning_log", "\n".join(missing))
        self.assertNotIn("missing_warning_log", "\n".join(governed))

    def test_default_driver_dataset_prefers_base(self):
        self.assertEqual(
            CONTROLS.driver({"base": "DM", "input": {"DM": "dm.csv", "VS": "vs.csv"}}),
            "DM",
        )
        self.assertEqual(
            CONTROLS.driver({"input": {"DM": "dm.csv"}}),
            "DM",
        )
        self.assertIsNone(CONTROLS.driver({"input": {"DM": "dm.csv", "VS": "vs.csv"}}))

    def test_rejects_lookup_pairing_and_verification_constraints(self):
        spec = {
            "domain": "ADSL",
            "input": {"DM": "dm.csv"},
            "base": "DM",
            "intermediates": [
                {"id": "FIRST", "dataset": "DM", "order_by": ["DM.DATE"]},
            ],
            "keys": ["USUBJID"],
            "output": {"columns": ["USUBJID"]},
            "columns": [
                {
                    "name": "USUBJID",
                    "derivation": {"source": "DM.USUBJID"},
                    "verifications": {"range": {}},
                }
            ],
            "verifications": [
                {"row_count": {}},
                {"all_or_none": {"id": "complete", "columns": ["USUBJID"]}},
                {"assert": {"id": "complete", "require": "TRUE"}},
            ],
        }

        errors = CONTROLS.contracts(spec, "example/spec.yaml")

        self.assertEqual([error.condition for error in errors], ["unpaired_fields"])
        self.assertEqual(
            errors[0].context,
            {"intermediate": "FIRST", "declared": ["order_by"], "missing": ["keep"]},
        )
        # Observe every original independent defect with valid surrounding
        # declarations. A prior pairing/schema refusal cannot stand in for the
        # row-count, cardinality, duplicate-id and column-type assertions.
        base = {
            "domain": "ADSL",
            "input": {"DM": "dm.csv"},
            "base": "DM",
            "keys": ["USUBJID"],
            "output": {"columns": ["USUBJID"]},
            "columns": [
                {
                    "name": "USUBJID",
                    "type": "str",
                    "derivation": {"source": "DM.USUBJID"},
                }
            ],
        }
        cases = [
            (
                {"verifications": [{"row_count": {}}]},
                "verifications[0].row_count",
                "invalid_declaration",
                {"reason": "row_count requires one bound"},
            ),
            (
                {
                    "verifications": [
                        {"all_or_none": {"id": "complete", "columns": ["USUBJID"]}}
                    ]
                },
                "verifications[0].all_or_none",
                "invalid_declaration",
                {"reason": "all_or_none names two distinct columns"},
            ),
            (
                {
                    "columns": [
                        base["columns"][0],
                        {"name": "AGE", "type": "int", "derivation": {"literal": 1}},
                    ],
                    "verifications": [
                        {
                            "all_or_none": {
                                "id": "complete",
                                "columns": ["USUBJID", "AGE"],
                            }
                        },
                        {"assert": {"id": "complete", "require": "TRUE"}},
                    ],
                },
                "verifications[1].assert",
                "duplicate_identifier",
                {
                    "reason": "verification id 'complete' repeats verifications[0].all_or_none"
                },
            ),
            (
                {
                    "columns": [
                        {**base["columns"][0], "verifications": [{"range": {"min": 1}}]}
                    ]
                },
                "columns.USUBJID.verifications[0].range",
                "invalid_declaration",
                {"reason": "a str column does not admit this verification"},
            ),
            (
                {
                    "columns": [
                        {
                            **base["columns"][0],
                            "type": "int",
                            "derivation": {"literal": 1},
                            "verifications": [{"range": {}}],
                        }
                    ]
                },
                "columns.USUBJID.verifications[0].range",
                "invalid_declaration",
                {"reason": "range requires one bound"},
            ),
        ]
        for changes, path, condition, context in cases:
            with self.subTest(path=path, context=context):
                [finding] = CONTROLS.contracts(
                    {**copy.deepcopy(base), **changes}, "example/spec.yaml"
                )
                self.assertEqual(finding.path, "example/spec.yaml." + path)
                self.assertEqual(finding.condition, condition)
                self.assertEqual(finding.context, context)

    def test_a_row_may_override_an_odm_column_derivation(self):
        # REQ-1260: the item read is a default in the row that uses its own
        # derivation instead.
        spec = {
            "domain": "DM",
            "input": {"ODM": "odm.csv"},
            "keys": ["USUBJID"],
            "output": {"columns": ["USUBJID", "SEX"]},
            "columns": [
                {"name": "USUBJID"},
                {"name": "SEX", "derivation": {"odm": "ODM.IT.DM.SEX"}},
            ],
            "rows": [
                {
                    "id": "subjects",
                    "dataset": "ODM",
                    "group_by": ["ODM.SubjectKey"],
                    "derivations": {
                        "USUBJID": "ODM.SubjectKey",
                        "SEX": {"literal": "U"},
                    },
                }
            ],
        }

        errors = CONTROLS.contracts(spec, "example/spec.yaml")

        duplicates = [
            error
            for error in errors
            if getattr(error, "condition", None) == "duplicate_derivation"
        ]
        self.assertEqual(duplicates, [])
        self.assertNotIn("missing_derivation", [error.condition for error in errors])

    def test_short_unique_still_checks_declared_columns(self):
        spec = {
            "domain": "ADSL",
            "input": {"DM": "dm.csv"},
            "base": "DM",
            "keys": ["USUBJID"],
            "output": {"columns": ["USUBJID"]},
            "columns": [{"name": "USUBJID", "derivation": {"source": "DM.USUBJID"}}],
            "verifications": [{"unique": ["MISSING"]}],
        }

        errors = CONTROLS.contracts(spec, "example/spec.yaml")

        self.assertEqual(errors[0].condition, "unknown_field")
        self.assertEqual(errors[0].context["reason"], "unknown column 'MISSING'")

    def test_accepts_grouped_row_count(self):
        spec = {
            "domain": "ADLB",
            "input": {"LB": "lb.csv"},
            "base": "LB",
            "keys": ["USUBJID"],
            "output": {"columns": ["USUBJID"]},
            "columns": [
                {"name": "USUBJID", "derivation": {"source": "LB.USUBJID"}},
                {"name": "ABLFL", "derivation": {"literal": "Y"}},
            ],
            "verifications": [
                {
                    "row_count": {
                        "id": "one-baseline-per-subject",
                        "group_by": ["USUBJID"],
                        "filter": "ABLFL = 'Y'",
                        "max": 1,
                    }
                }
            ],
        }

        self.assertEqual(CONTROLS.contracts(spec, "example/spec.yaml"), [])

    def test_row_count_fraction_bounds(self):
        spec = {
            "domain": "OUT",
            "input": {"DM": "dm.csv"},
            "base": "DM",
            "keys": ["USUBJID"],
            "output": {"columns": ["USUBJID"]},
            "columns": [
                {"name": "USUBJID", "derivation": {"source": "DM.USUBJID"}},
                {"name": "VAL", "derivation": {"literal": "x"}},
            ],
            "verifications": [
                {
                    "row_count": {
                        "id": "missing-rate",
                        "filter": "VAL IS NULL",
                        "max_fraction": 0.05,
                    }
                }
            ],
        }
        self.assertEqual(CONTROLS.contracts(spec, "example/spec.yaml"), [])

        spec["verifications"][0]["row_count"]["max_fraction"] = 1.1
        errors = CONTROLS.contracts(spec, "example/spec.yaml")
        self.assertEqual(
            errors[0].path, "example/spec.yaml.verifications[0].row_count.max_fraction"
        )
        self.assertEqual(
            errors[0].context["reason"], "a row_count fraction is between 0 and 1"
        )

        spec["verifications"][0]["row_count"].update(
            {"min_fraction": 0.75, "max_fraction": 0.25}
        )
        errors = CONTROLS.contracts(spec, "example/spec.yaml")
        self.assertIn("fraction", errors[0].context["reason"])
        self.assertIn("max", errors[0].context["reason"])

    def test_accepts_grouped_row_count_without_id_but_rejects_bad_columns(self):
        spec = {
            "domain": "ADLB",
            "input": {"LB": "lb.csv"},
            "base": "LB",
            "keys": ["USUBJID"],
            "output": {"columns": ["USUBJID"]},
            "columns": [{"name": "USUBJID", "derivation": {"source": "LB.USUBJID"}}],
            "verifications": [
                {
                    "row_count": {
                        "group_by": ["USUBJID", "USUBJID", "MISSING"],
                        "max": 1,
                    }
                },
                {"row_count": {"id": "empty-grain", "group_by": [], "max": 1}},
            ],
        }

        errors = CONTROLS.contracts(spec, "example/spec.yaml")

        message = "\n".join(errors)
        self.assertNotIn("missing_verification_id", message)
        self.assertIn("row_count group_by repeats a column", message)
        spec["verifications"][0]["row_count"]["group_by"] = ["MISSING"]
        self.assertIn(
            "unknown column 'MISSING'",
            "\n".join(CONTROLS.contracts(spec, "example/spec.yaml")),
        )
        spec["verifications"] = [spec["verifications"][1]]
        empty = CONTROLS.contracts(spec, "example/spec.yaml")
        self.assertEqual(empty[0].condition, "invalid_declaration")
        self.assertIn("group_by names at least one column", empty[0].context["reason"])

    def test_rejects_duplicate_row_count_verification_id(self):
        spec = {
            "domain": "ADLB",
            "input": {"LB": "lb.csv"},
            "base": "LB",
            "keys": ["USUBJID"],
            "output": {"columns": ["USUBJID"]},
            "columns": [{"name": "USUBJID", "derivation": {"source": "LB.USUBJID"}}],
            "verifications": [
                {
                    "row_count": {
                        "id": "one-row-per-subject",
                        "group_by": ["USUBJID"],
                        "max": 1,
                    }
                },
                {
                    "assert": {
                        "id": "one-row-per-subject",
                        "require": "TRUE",
                    }
                },
            ],
        }

        errors = CONTROLS.contracts(spec, "example/spec.yaml")

        self.assertIn("duplicate_identifier", "\n".join(errors))

    def test_rejects_a_source_extension_that_names_no_profile(self):
        snapshots = SOURCE_CONTROLS.Captures()
        with tempfile.TemporaryDirectory() as temp_dir:
            example_dir = Path(temp_dir)
            input_dir = example_dir / "input"
            input_dir.mkdir()
            (input_dir / "dm.txt").write_text("USUBJID\n01\n")
            spec_path = example_dir / "spec.yaml"
            spec = {
                "domain": "ADSL",
                "input": {"DM": "input/dm.txt"},
                "base": "DM",
                "keys": ["USUBJID"],
                "output": {"columns": ["USUBJID"]},
                "columns": [
                    {
                        "name": "USUBJID",
                        "derivation": {"source": "DM.USUBJID"},
                    }
                ],
            }

            errors = CONTROLS.contracts(
                spec, "example/spec.yaml", spec_path, snapshots=snapshots
            )

        message = "\n".join(errors)
        self.assertIn("source_profile_unknown", message)
        self.assertIn("input/dm.txt", message)
        # R023 decides the profile from the written path, so the source is
        # never opened to reject it.
        self.assertEqual(snapshots.reads, 0)
        diagnostic = next(
            error
            for error in errors
            if getattr(error, "condition", None) == "source_profile_unknown"
        )
        self.assertEqual(diagnostic.path, "example/spec.yaml.input.DM.path")
        self.assertEqual(diagnostic.context, {"dataset": "DM", "path": "input/dm.txt"})

    def test_accepts_parquet_as_a_source_profile(self):
        snapshots = SOURCE_CONTROLS.Captures()
        with tempfile.TemporaryDirectory() as temp_dir:
            example_dir = Path(temp_dir)
            input_dir = example_dir / "input"
            input_dir.mkdir()
            # Static repository validation selects the profile and captures
            # the file. The runtime owns R027 container ingestion.
            (input_dir / "dm.parquet").write_bytes(b"PAR1")
            spec_path = example_dir / "spec.yaml"
            spec = {
                "domain": "ADSL",
                "input": {"DM": "input/dm.parquet"},
                "base": "DM",
                "keys": ["USUBJID"],
                "output": {"columns": ["USUBJID"]},
                "columns": [{"name": "USUBJID", "derivation": {"literal": "fixture"}}],
            }

            errors = CONTROLS.contracts(
                spec, "example/spec.yaml", spec_path, snapshots=snapshots
            )

        self.assertNotIn("source_profile_unknown", "\n".join(errors))
        self.assertEqual(snapshots.reads, 1)

    def test_rejects_inline_types_for_a_parquet_source(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            example_dir = Path(temp_dir)
            input_dir = example_dir / "input"
            input_dir.mkdir()
            (input_dir / "dm.parquet").write_bytes(b"PAR1")
            spec_path = example_dir / "spec.yaml"
            spec = {
                "domain": "ADSL",
                "input": {
                    "DM": {
                        "path": "input/dm.parquet",
                        "types": {"USUBJID": "str"},
                    }
                },
                "base": "DM",
                "keys": ["USUBJID"],
                "output": {"columns": ["USUBJID"]},
                "columns": [{"name": "USUBJID", "derivation": {"literal": "fixture"}}],
            }

            errors = CONTROLS.contracts(spec, "example/spec.yaml", spec_path)

        diagnostic = next(
            error
            for error in errors
            if getattr(error, "condition", None) == "redundant_field_type"
        )
        self.assertEqual(diagnostic.path, "example/spec.yaml.input.DM.types.USUBJID")
        self.assertEqual(
            diagnostic.context,
            {"dataset": "DM", "field": "USUBJID", "type": "str"},
        )

    def test_rejects_missing_source_and_type_for_absent_csv_field(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            example_dir = Path(temp_dir)
            input_dir = example_dir / "input"
            input_dir.mkdir()
            (input_dir / "dm.csv").write_text("USUBJID\n01\n")
            spec_path = example_dir / "spec.yaml"
            spec = {
                "domain": "ADSL",
                "input": {
                    "DM": {
                        "path": "input/dm.csv",
                        "types": {"AGE": "int"},
                    },
                    "EX": "input/missing.csv",
                },
                "base": "DM",
                "keys": ["USUBJID"],
                "output": {"columns": ["USUBJID"]},
                "columns": [
                    {
                        "name": "USUBJID",
                        "derivation": {"source": "DM.USUBJID"},
                    }
                ],
            }

            errors = CONTROLS.contracts(spec, "example/spec.yaml", spec_path)
            del spec["input"]["EX"]
            type_errors = CONTROLS.contracts(spec, "example/spec.yaml", spec_path)

        self.assertIn("types.AGE", "\n".join(type_errors))
        message = "\n".join(errors)
        self.assertIn(
            "input.EX.path: resource_path_missing: 'input/missing.csv' does not exist",
            message,
        )

    def test_accepts_absent_artifact_its_producer_writes(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            example_dir = Path(temp_dir)
            spec_path = example_dir / "spec_supp.yaml"
            spec = {
                "domain": "SUPP",
                "input": {
                    "DM": {"path": "dm.csv", "schema": "spec_dm.yaml"},
                },
                "keys": ["USUBJID"],
                "output": {"columns": ["USUBJID"]},
                "columns": [
                    {
                        "name": "USUBJID",
                        "derivation": {"source": "DM.USUBJID"},
                    }
                ],
            }

            errors = CONTROLS.contracts(spec, "example/spec_supp.yaml", spec_path)

        self.assertNotIn("input.DM.path", "\n".join(errors))


class TestProducingSpecs(unittest.TestCase):
    VALID_PRODUCER_SPEC = """schema_version: "1.0"
domain: DM
input:
  RAW: raw.csv
base: RAW
keys: [STUDYID]
output:
  path: dm.csv
  columns: [STUDYID, AGE]
columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: {source: RAW.STUDYID}
  - name: AGE
    type: int
    label: Age
    derivation: {source: RAW.AGE}
"""

    def setUp(self):
        self.env, schema_errors = OWNER_REPOSITORY_SCHEMA.build_schema_env(ROOT)
        self.assertEqual(schema_errors, [])
        self.test_dir = tempfile.TemporaryDirectory()
        self.example_dir = Path(self.test_dir.name)
        self.input_dir = self.example_dir / "input"
        self.input_dir.mkdir()
        self.spec_path = self.example_dir / "spec.yaml"
        (self.input_dir / "dm.csv").write_text(
            "STUDYID,AGE\nSTUDY1,42\n", encoding="utf-8"
        )
        (self.input_dir / "raw.csv").write_text(
            "STUDYID,AGE\nSTUDY1,42\n", encoding="utf-8"
        )

    def tearDown(self):
        self.test_dir.cleanup()

    def write_producer_spec(self, content=None):
        if content is None:
            content = self.VALID_PRODUCER_SPEC
        (self.input_dir / "dm.schema.yaml").write_text(content, encoding="utf-8")

    def validate(self, source=None):
        if source is None:
            source = {
                "path": "input/dm.csv",
                "schema": "input/dm.schema.yaml",
            }
        return SOURCE_CONTROLS.producing(
            {"input": {"DM": source}},
            "example/spec.yaml",
            self.spec_path,
            self.env,
            {self.spec_path.resolve()},
        )

    def test_accepts_complete_producing_spec(self):
        self.write_producer_spec()

        self.assertEqual(self.validate(), [])

    def test_absent_artifact_ordinal_checks_the_producer_output_contract(self):
        """Reject known CSV/Parquet field collisions before production runs."""
        for profile in ("csv", "parquet"):
            for ordinal in ("AGE", "OriginalPosition"):
                with self.subTest(profile=profile, ordinal=ordinal):
                    self.write_producer_spec(
                        self.VALID_PRODUCER_SPEC.replace(
                            "path: dm.csv", f"path: absent.{profile}"
                        )
                    )
                    source = {
                        "path": f"input/absent.{profile}",
                        "schema": "input/dm.schema.yaml",
                        "ordinal": ordinal,
                    }
                    consumer = {
                        "schema_version": "1.0",
                        "domain": "ADSL",
                        "keys": ["STUDYID"],
                        "input": {"DM": source},
                        "output": {"path": "adsl.csv", "columns": ["STUDYID"]},
                        "columns": [
                            {
                                "name": "STUDYID",
                                "type": "str",
                                "label": "Study Identifier",
                                "derivation": "DM.STUDYID",
                            }
                        ],
                    }
                    findings = OWNER_REPOSITORY_SCHEMA.validate_spec_document(
                        consumer, "example/spec.yaml", self.spec_path, self.env
                    )
                    if ordinal == "OriginalPosition":
                        self.assertEqual(findings, [])
                    else:
                        [finding] = findings
                        self.assertEqual(finding.condition, "ordinal_field_collision")
                        self.assertEqual(
                            finding.path, "example/spec.yaml.input.DM.ordinal"
                        )
                        self.assertEqual(
                            finding.context, {"dataset": "DM", "field": "AGE"}
                        )

    def test_internal_producer_column_does_not_shadow_generated_ordinal(self):
        """Only published producer fields constrain consumer ordinal names."""
        producer = yaml.safe_load(self.VALID_PRODUCER_SPEC)
        producer["columns"].append(
            {
                "name": "OriginalPosition",
                "type": "str",
                "label": "Internal Position",
                "derivation": {"literal": "private"},
            }
        )
        self.write_producer_spec(yaml.safe_dump(producer, sort_keys=False))
        source = {
            "path": "input/absent.csv",
            "schema": "input/dm.schema.yaml",
            "ordinal": "OriginalPosition",
        }
        self.assertEqual(self.validate(source), [])
        catalog = SOURCE_CONTROLS.catalog(
            {"input": {"DM": source}}, self.spec_path, self.env
        )
        self.assertEqual(catalog["DM"]["OriginalPosition"], "int")
        del source["ordinal"]
        catalog = SOURCE_CONTROLS.catalog(
            {"input": {"DM": source}}, self.spec_path, self.env
        )
        self.assertNotIn("OriginalPosition", catalog["DM"])

    def test_ordinal_collision_uses_the_resolved_producer_contract(self):
        """Inherited output fields reject ordinal collisions before ingestion."""
        self.write_producer_spec()
        (self.input_dir / "child.schema.yaml").write_text(
            'schema_version: "1.0"\nparents: dm.schema.yaml\n',
            encoding="ascii",
        )
        [finding] = self.validate(
            {
                "path": "input/absent.csv",
                "schema": "input/child.schema.yaml",
                "ordinal": "AGE",
            }
        )
        self.assertEqual(finding.condition, "ordinal_field_collision")
        self.assertEqual(finding.context, {"dataset": "DM", "field": "AGE"})

    def test_rejects_inline_types_with_producing_spec(self):
        self.write_producer_spec()

        errors = self.validate(
            {
                "path": "input/dm.csv",
                "schema": "input/dm.schema.yaml",
                "types": {"AGE": "int"},
            }
        )

        self.assertIn("example/spec.yaml.input.DM.types.AGE", "\n".join(errors))

        empty_types_errors = self.validate(
            {
                "path": "input/dm.csv",
                "schema": "input/dm.schema.yaml",
                "types": {},
            }
        )
        self.assertEqual(empty_types_errors[0].condition, "redundant_field_type")
        self.assertEqual(empty_types_errors[0].path, "example/spec.yaml.input.DM.types")

    def test_rejects_producer_version_mismatch(self):
        self.write_producer_spec(
            self.VALID_PRODUCER_SPEC.replace(
                'schema_version: "1.0"', 'schema_version: "2.0"'
            )
        )

        message = "\n".join(self.validate())

        self.assertIn("schema_version_mismatch", message)

    def test_rejects_reordered_producer_output(self):
        self.write_producer_spec(
            """schema_version: "1.0"
domain: DM
input: {RAW: raw.csv}
base: RAW
keys: [STUDYID]
output:
  path: dm.csv
  columns: [AGE, STUDYID]
columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: {source: RAW.STUDYID}
  - name: AGE
    type: int
    label: Age
    derivation: {source: RAW.AGE}
"""
        )

        message = "\n".join(self.validate())

        self.assertIn("match the artifact header exactly", message)
        self.assertIn("['STUDYID', 'AGE']", message)
        self.assertIn("['AGE', 'STUDYID']", message)

    def test_rejects_empty_producer_artifact(self):
        self.write_producer_spec()
        (self.input_dir / "dm.csv").write_text("", encoding="utf-8")

        message = "\n".join(self.validate())

        self.assertIn("stored CSV artifact is empty", message)

    def test_rejects_legacy_field_map(self):
        self.write_producer_spec(
            """version: "1.0"
fields: {STUDYID: string, AGE: integer}
"""
        )

        original = yaml.safe_load((self.input_dir / "dm.schema.yaml").read_text())
        partial = typed_schema_control(
            original,
            "producer",
            None,
            {
                "classes": {
                    "root_class": [
                        {"schema_version": {"type": "str", "required": True}}
                    ]
                }
            },
        )
        message = "\n".join(self.validate())
        self.assertIn("schema_version_mismatch", message)
        message = "\n".join(partial)

        self.assertIn("missing_required_field", message)
        self.assertIn("producer.version: unknown_field", message)
        self.assertIn("producer.fields: unknown_field", message)

    def test_rejects_metadata_only_contract(self):
        self.write_producer_spec(
            """schema_version: "1.0"
domain: DM
input: {}
keys: [STUDYID]
output: {path: out.csv, columns: [STUDYID, AGE]}
columns:
  - name: STUDYID
    type: str
    label: Study Identifier
  - name: AGE
    type: int
    label: Age
"""
        )

        message = "\n".join(self.validate())

        self.assertIn("driver_unavailable", message)
        self.assertIn("missing_derivation", message)

    def test_rejects_unlabeled_producer_output(self):
        self.write_producer_spec(
            self.VALID_PRODUCER_SPEC.replace("label: Age", "label: ''")
        )

        message = "\n".join(self.validate())

        self.assertIn("invalid_producer_contract", message)
        self.assertIn("columns.AGE.label", message)

    def test_rejects_missing_producer_input(self):
        self.write_producer_spec()
        (self.input_dir / "raw.csv").unlink()

        message = "\n".join(self.validate())

        self.assertIn("resource_path_missing: 'raw.csv' does not exist", message)

    def test_rejects_producer_dependency_cycle(self):
        looping_producer = """schema_version: "1.0"
domain: {domain}
input:
  LOOP:
    path: out.csv
    schema: {link}
base: LOOP
keys: [STUDYID]
output: {{path: out.csv, columns: [STUDYID, AGE]}}
columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: {{source: LOOP.STUDYID}}
  - name: AGE
    type: int
    label: Age
    derivation: {{source: LOOP.AGE}}
"""
        self.write_producer_spec(
            looping_producer.format(domain="DM", link="other.yaml")
        )
        (self.input_dir / "other.yaml").write_text(
            looping_producer.format(domain="OTHER", link="dm.schema.yaml"),
            encoding="utf-8",
        )

        message = "\n".join(self.validate())

        self.assertIn("producer_workflow_cycle", message)

    def test_reports_a_missing_parent_relative_producer_source(self):
        self.write_producer_spec(
            self.VALID_PRODUCER_SPEC.replace(
                "input:\n  RAW: raw.csv",
                "input:\n  RAW: ../../escape.csv",
            )
        )

        message = "\n".join(self.validate())

        self.assertIn(
            "resource_path_missing: '../../escape.csv' does not exist",
            message,
        )

    def test_rejects_missing_producing_spec(self):
        message = "\n".join(self.validate())

        self.assertIn(
            "input.DM.schema: resource_path_missing: "
            "'input/dm.schema.yaml' does not exist",
            message,
        )


class TestProjectResourceBoundary(unittest.TestCase):
    """R021: declaring-file paths retain kind/link/held-byte checks."""

    def setUp(self):
        self.test_dir = tempfile.TemporaryDirectory()
        self.root = Path(self.test_dir.name)
        self.input_dir = self.root / "input"
        self.input_dir.mkdir()
        self.source = self.input_dir / "dm.csv"
        self.source.write_text("STUDYID,AGE\nSTUDY1,42\n", encoding="utf-8")

    def tearDown(self):
        self.test_dir.cleanup()

    def resolve(self, written, base_dir=None):
        return SOURCE_CONTROLS.resource(
            written,
            self.root if base_dir is None else base_dir,
        )

    def test_accepts_nested_in_project_file(self):
        accepted, condition = self.resolve("input/dm.csv")
        self.assertIsNone(condition)
        self.assertEqual(accepted.read_bytes(), self.source.read_bytes())

    def test_accepts_absolute_paths_outside_the_declaring_directory(self):
        # REQ-0774: no project/data-root declaration is needed.
        declaring = self.root / "study"
        declaring.mkdir()
        accepted, condition = self.resolve(str(self.source.resolve()), declaring)
        self.assertIsNone(condition)
        self.assertEqual(accepted.read_bytes(), self.source.read_bytes())
        self.assertEqual(
            self.resolve(str(self.source.resolve()) + ".absent", declaring)[1],
            "resource_path_missing",
        )

    def test_accepts_parent_relative_paths_without_configuration(self):
        # REQ-0778/0780: the original declaring file supplies the only origin.
        declaring = self.root / "study"
        declaring.mkdir()
        accepted, condition = self.resolve("../input/dm.csv", declaring)
        self.assertIsNone(condition)
        self.assertEqual(accepted.resolve(), self.source.resolve())

    def test_accepts_the_caller_selected_declaring_directory_alias(self):
        # REQ-0781 exempts only the selected declaring directory's alias.
        linked = self.root / "selected"
        linked.symlink_to(self.input_dir, target_is_directory=True)
        for written in ("dm.csv", str(linked / "dm.csv")):
            accepted, condition = self.resolve(written, linked)
            self.assertIsNone(condition)
            self.assertEqual(accepted.resolve(), self.source.resolve())

    def test_rejects_declared_directory_aliases_outside_the_selected_anchor(self):
        # REQ-0783: declarations cannot select additional exempt directories.
        linked = self.root / "declared"
        linked.symlink_to(self.input_dir, target_is_directory=True)
        self.assertEqual(
            self.resolve(str(linked / "dm.csv"))[1], "resource_path_symlink"
        )

    def test_rooted_and_relative_spellings_reach_one_file(self):
        rooted, rooted_condition = self.resolve(f"{self.root.as_posix()}/input/dm.csv")
        relative, relative_condition = self.resolve("input/dm.csv")

        self.assertIsNone(rooted_condition)
        self.assertIsNone(relative_condition)
        self.assertEqual(rooted.resolve(), relative.resolve())

    def test_rejects_uri_schemes(self):
        for written in (
            "https://example.org/dm.csv",
            "file:///etc/passwd",
            "s3://bucket/dm.csv",
            "C:data/dm.csv",
        ):
            with self.subTest(written=written):
                self.assertEqual(self.resolve(written)[1], "resource_path_uri_scheme")

    def test_traversal_within_root_resolves(self):
        accepted, condition = self.resolve("input/../input/dm.csv")
        self.assertIsNone(condition)
        self.assertEqual(accepted.name, "dm.csv")

    def test_missing_parent_relative_paths_are_missing_without_containment(self):
        declaring = self.root / "study"
        declaring.mkdir()
        for written in ("../absent.csv", "nested/../../absent.csv"):
            self.assertEqual(
                self.resolve(written, declaring)[1], "resource_path_missing"
            )
        self.assertEqual(
            self.resolve("..", declaring)[1], "resource_path_not_regular_file"
        )

    def test_rejects_unnormalized_written_forms(self):
        for written in (
            "input//dm.csv",
            "input/dm.csv/",
            "input\\dm.csv",
            "",
            "/",
        ):
            with self.subTest(written=written):
                self.assertEqual(
                    self.resolve(written)[1], "resource_path_not_normalized"
                )

    def test_rejects_symlinked_file_inside_the_project(self):
        link = self.input_dir / "linked.csv"
        link.symlink_to("dm.csv")

        self.assertEqual(self.resolve("input/linked.csv")[1], "resource_path_symlink")

    def test_rejects_symlinked_directory_component(self):
        (self.input_dir / "nested").mkdir()
        (self.input_dir / "nested" / "dm.csv").write_text(
            "STUDYID\nSTUDY1\n", encoding="utf-8"
        )
        (self.root / "linked").symlink_to("input/nested")

        self.assertEqual(self.resolve("linked/dm.csv")[1], "resource_path_symlink")

    def test_rejects_symlink_escaping_the_project(self):
        outside = Path(self.test_dir.name).parent / "outside_dm.csv"
        outside.write_text("STUDYID\nSTUDY1\n", encoding="utf-8")
        self.addCleanup(outside.unlink)
        (self.input_dir / "escape.csv").symlink_to(outside)

        self.assertEqual(self.resolve("input/escape.csv")[1], "resource_path_symlink")

    def test_rejects_directory(self):
        self.assertEqual(self.resolve("input")[1], "resource_path_not_regular_file")

    def test_rejects_file_used_as_a_directory_component(self):
        self.assertEqual(
            self.resolve("input/dm.csv/nested.csv")[1],
            "resource_path_not_regular_file",
        )

    @unittest.skipUnless(hasattr(os, "mkfifo"), "requires POSIX FIFOs")
    def test_rejects_fifo(self):
        os.mkfifo(self.input_dir / "stream.csv")

        self.assertEqual(
            self.resolve("input/stream.csv")[1],
            "resource_path_not_regular_file",
        )

    def test_rejects_socket(self):
        endpoint = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.addCleanup(endpoint.close)
        endpoint.bind(str(self.input_dir / "socket.csv"))

        self.assertEqual(
            self.resolve("input/socket.csv")[1],
            "resource_path_not_regular_file",
        )

    def test_rejects_missing_file(self):
        self.assertEqual(self.resolve("input/absent.csv")[1], "resource_path_missing")

    def test_absolute_dot_segments_normalize_before_walking(self):
        # REQ-0778: an elided component is never a filesystem authority.
        written = str(self.root / "absent" / ".." / "input" / "dm.csv")
        accepted, condition = self.resolve(written)
        self.assertIsNone(condition)
        self.assertEqual(accepted.resolve(), self.source.resolve())

    def test_missing_declaring_file_path_never_retries_the_entry_directory(self):
        # REQ-1246: the existing entry-directory file cannot satisfy this declaration.
        nested = self.root / "common"
        nested.mkdir()
        self.assertEqual(
            self.resolve("input/dm.csv", nested)[1], "resource_path_missing"
        )
        accepted, condition = self.resolve("../input/dm.csv", nested)
        self.assertIsNone(condition)
        self.assertEqual(accepted.resolve(), self.source.resolve())

    def test_parent_steps_at_filesystem_root_stay_at_root(self):
        # REQ-0778: parent traversal is textual even above the filesystem root.
        written = "/../../" + self.source.resolve().as_posix().lstrip("/")
        accepted, condition = self.resolve(written)
        self.assertIsNone(condition)
        self.assertEqual(accepted.resolve(), self.source.resolve())

    def test_error_names_the_written_path_and_no_host_path(self):
        message = SOURCE_CONTROLS.resource_findings(
            "input/absent.csv", self.root, "spec.yaml.input.DM.path"
        )[0]

        self.assertIn("resource_path_missing", message)
        self.assertIn("'input/absent.csv'", message)
        self.assertNotIn(str(self.root), message)

    def test_repeated_declarations_share_one_snapshot(self):
        snapshots = SOURCE_CONTROLS.Captures()

        first, first_condition = snapshots.read(self.source)
        second, second_condition = snapshots.read(self.source)

        self.assertIsNone(first_condition)
        self.assertIsNone(second_condition)
        self.assertTrue(second.same_snapshot)
        self.assertEqual(snapshots.reads, 1)
        self.assertEqual(first.csv_header(), ["STUDYID", "AGE"])

    def test_replaced_content_fails_after_validation(self):
        snapshots = SOURCE_CONTROLS.Captures()
        accepted, condition = snapshots.read(self.source)
        self.assertIsNone(condition)

        self.source.write_text("STUDYID,AGE\nSTUDY2,7\n", encoding="utf-8")

        replaced, condition = snapshots.read(self.source)

        self.assertIsNone(replaced)
        self.assertEqual(condition, "resource_path_content_changed")
        self.assertEqual(accepted.csv_header(), ["STUDYID", "AGE"])


class TestRetiredProjectConfiguration(unittest.TestCase):
    def test_old_root_configuration_does_not_participate_in_resolution(self):
        # REQ-0772/1246: a malformed legacy configuration is never discovered.
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            declaring = root / "study"
            declaring.mkdir()
            (declaring / "yamaa-project.yaml").write_bytes(b"invalid: [")
            source = root / "dm.csv"
            source.write_text("ID\n1\n")
            accepted, condition = SOURCE_CONTROLS.resource("../dm.csv", declaring)
            self.assertIsNone(condition)
            self.assertEqual(accepted.read_bytes(), source.read_bytes())


class TestProjectResourceBoundaryInSpecs(unittest.TestCase):
    """R021 as the specification validator applies it."""

    def setUp(self):
        self.env, schema_errors = OWNER_REPOSITORY_SCHEMA.build_schema_env(ROOT)
        self.assertEqual(schema_errors, [])
        self.test_dir = tempfile.TemporaryDirectory()
        self.example_dir = Path(self.test_dir.name)
        self.input_dir = self.example_dir / "input"
        self.input_dir.mkdir()
        (self.input_dir / "dm.csv").write_text(
            "STUDYID,USUBJID\nSTUDY1,S1\n", encoding="utf-8"
        )
        self.spec_path = self.example_dir / "spec.yaml"

    def tearDown(self):
        self.test_dir.cleanup()

    def spec(self, source):
        return {
            "domain": "ADSL",
            "input": {"DM": "input/dm.csv", "REF": source},
            "base": "DM",
            "keys": ["USUBJID"],
            "output": {"columns": ["USUBJID"]},
            "columns": [{"name": "USUBJID", "derivation": {"source": "DM.USUBJID"}}],
        }

    def contracts(self, source, project_root=None):
        return CONTROLS.contracts(
            self.spec(source),
            "example/spec.yaml",
            self.spec_path,
            project_root,
        )

    def test_reports_each_rejection_at_the_declaring_field(self):
        cases = {
            "../missing-declared-ref.csv": "resource_path_missing",
            "https://example.org/ref.csv": "resource_path_uri_scheme",
            "input": "resource_path_not_regular_file",
            "input/absent.csv": "resource_path_missing",
        }
        for written, condition in cases.items():
            with self.subTest(written=written):
                message = "\n".join(self.contracts(written))
                self.assertIn(
                    f"example/spec.yaml.input.REF.path: {condition}: {written!r}",
                    message,
                )
                self.assertNotIn(str(self.example_dir), message)

    def test_reports_a_symlinked_source(self):
        (self.input_dir / "ref.csv").symlink_to("dm.csv")

        message = "\n".join(self.contracts("input/ref.csv"))

        self.assertIn(
            "input.REF.path: resource_path_symlink: 'input/ref.csv'",
            message,
        )

    def test_accepts_a_nested_in_project_source(self):
        (self.input_dir / "nested").mkdir()
        (self.input_dir / "nested" / "ref.csv").write_text(
            "STUDYID\nSTUDY1\n", encoding="utf-8"
        )

        self.assertEqual(self.contracts("input/nested/ref.csv"), [])

    def test_accepts_one_file_declared_under_two_identifiers(self):
        self.assertEqual(self.contracts("input/dm.csv"), [])

    def test_accepts_parent_relative_source_from_the_declaring_file(self):
        # REQ-0780: no project or data-root configuration is consulted.
        shared = self.example_dir.parent / (self.example_dir.name + "-shared.csv")
        try:
            shared.write_text("STUDYID\nSTUDY1\n", encoding="utf-8")
            self.assertEqual(self.contracts("../" + shared.name), [])
        finally:
            shared.unlink(missing_ok=True)

    def test_accepts_absolute_source_outside_the_declaring_directory(self):
        # REQ-0774/0784: an absolute location needs no containment grant.
        with tempfile.TemporaryDirectory() as directory:
            shared = Path(directory) / "ref.csv"
            shared.write_text("STUDYID\nSTUDY1\n", encoding="utf-8")
            self.assertEqual(self.contracts(shared.resolve().as_posix()), [])

    def test_legacy_project_root_argument_cannot_redirect_declaring_file_paths(self):
        confined = self.example_dir / "project"
        confined.mkdir()
        self.assertEqual(self.contracts("input/dm.csv", project_root=confined), [])

    def test_type_catalog_does_not_read_a_rejected_path(self):
        outside = self.example_dir / "outside.csv"
        outside.write_text("SECRET\nvalue\n", encoding="utf-8")

        catalog = SOURCE_CONTROLS.catalog(
            self.spec("../outside.csv"), self.spec_path, self.env
        )

        self.assertEqual(catalog.get("REF"), {})


class TestRegularExpressionContract(unittest.TestCase):
    """R022: the portable contract binding reads every pattern."""

    @classmethod
    def setUpClass(cls):
        cls.root = ROOT
        cls.env, schema_errors = OWNER_REPOSITORY_SCHEMA.build_schema_env(cls.root)
        assert not schema_errors, schema_errors

    # -- the portable contract ---------------------------------------------

    def test_the_validator_names_the_contract_it_replays(self):
        self.assertEqual(OWNER_REGEX_CHECKS.REGEX_CONTRACT, "regex")
        self.assertEqual(OWNER_REGEX_CHECKS.REGEX_CONTRACT_VERSION, "2.0.0")

    def test_the_replay_reads_through_public_native_specifications(self):
        OWNER_REGEX_CHECKS.require_regex_binding()
        from tools.validation import _native_regex as package_binding

        self.assertIs(
            OWNER_REGEX_CHECKS._portable_compile, package_binding.compile_pattern
        )
        self.assertIs(
            OWNER_REGEX_CHECKS._portable_full_match, package_binding.full_match
        )
        self.assertIs(package_binding.check, __import__("yamaa").check)
        self.assertIs(package_binding.domain, __import__("yamaa").domain)

    def test_missing_binding_fails_rather_than_falling_back(self):
        saved = OWNER_REGEX_CHECKS._portable_binding
        try:
            OWNER_REGEX_CHECKS._portable_binding = False
            with self.assertRaises(
                OWNER_REGEX_CHECKS.RegexBindingUnavailable
            ) as caught:
                OWNER_REGEX_CHECKS.compile_regex("a")
        finally:
            OWNER_REGEX_CHECKS._portable_binding = saved
        message = str(caught.exception)
        self.assertIn("unsupported_regex_engine", message)
        self.assertIn(OWNER_REGEX_CHECKS.REGEX_CONTRACT_VERSION, message)

    # -- the contract is not the host default ------------------------------

    def test_accepts_contract_syntax_beyond_the_host_library(self):
        for pattern in ("(?<name>a)", r"\u{1D400}", "(?<=ab)x"):
            with self.subTest(pattern=pattern):
                OWNER_REGEX_CHECKS.compile_regex(pattern)

    def test_rejects_syntax_outside_the_portable_contract(self):
        for pattern in (
            "(?P<name>a)",
            "(?i)a",
            r"\a",
            "a{",
            "(?<=a+)b",
            r"\p{L}",
        ):
            with self.subTest(pattern=pattern):
                with self.assertRaises(OWNER_REGEX_CHECKS.InvalidRegex):
                    OWNER_REGEX_CHECKS.compile_regex(pattern)

    def test_character_classes_do_not_widen_to_unicode(self):
        self.assertTrue(OWNER_REGEX_CHECKS.regex_full_match(r"\d", "5"))
        self.assertFalse(OWNER_REGEX_CHECKS.regex_full_match(r"\d", chr(0x0665)))
        self.assertFalse(OWNER_REGEX_CHECKS.regex_full_match(r"\w", chr(0x00E9)))
        with self.assertRaises(OWNER_REGEX_CHECKS.InvalidRegex):
            OWNER_REGEX_CHECKS.regex_full_match(r"\p{L}", chr(0x00E9))

    def test_dollar_does_not_match_before_a_trailing_line_feed(self):
        self.assertTrue(OWNER_REGEX_CHECKS.regex_full_match("a", "a"))
        self.assertFalse(OWNER_REGEX_CHECKS.regex_full_match("a", "a\n"))
        self.assertFalse(OWNER_REGEX_CHECKS.regex_full_match("a$", "a\n"))

    def test_a_supplementary_scalar_is_one_character(self):
        scalar = chr(0x1D400)
        self.assertTrue(OWNER_REGEX_CHECKS.regex_full_match(".", scalar))
        self.assertFalse(OWNER_REGEX_CHECKS.regex_full_match("..", scalar))
        self.assertTrue(OWNER_REGEX_CHECKS.regex_full_match(r"\u{1D400}", scalar))

    def test_matching_applies_no_normalization(self):
        self.assertFalse(
            OWNER_REGEX_CHECKS.regex_full_match(r"\u{00E9}", "e" + chr(0x0301))
        )

    # -- full match versus search ------------------------------------------

    def test_the_pattern_keyword_is_a_full_match_and_matches_searches(self):
        self.assertFalse(OWNER_REGEX_CHECKS.regex_full_match("[0-9]{3}", "AE-007-X"))
        self.assertIsNotNone(OWNER_REGEX_CHECKS.regex_search("[0-9]{3}", "AE-007-X"))

    def test_anchoring_adds_no_capture_group(self):
        source = OWNER_REGEX_CHECKS.anchored_regex_source("(a)|(b)")
        self.assertEqual(source, "^(?:(a)|(b))$")
        self.assertEqual(OWNER_REGEX_CHECKS.regex_capture_group_count(source), 2)
        self.assertTrue(OWNER_REGEX_CHECKS.regex_full_match("a|ab", "ab"))

    def test_every_fixture_pattern_survives_anchoring(self):
        document = yaml.safe_load(
            (self.root / "yaml" / "conformance" / "regex.yaml").read_text()
        )
        for case in document["cases"]:
            if case.get("invalid"):
                continue
            with self.subTest(case=case["id"]):
                OWNER_REGEX_CHECKS.compile_regex(
                    OWNER_REGEX_CHECKS.anchored_regex_source(case["pattern"])
                )

    # -- capture groups -----------------------------------------------------

    def test_counts_capturing_groups_by_opening_parenthesis(self):
        cases = {
            "^A-([0-9]+)$": 1,
            "^(?:A)-([0-9]+)$": 1,
            "^(?<p>[A-Z]+)-(?<s>[0-9]+)$": 2,
            "^((a)(b))$": 3,
            "^(?=A)([A-Z]+)$": 1,
            "(?<!x)(y)": 1,
            "[(]": 0,
            r"[\]()]": 0,
            r"\((a)\)": 1,
            "((((a))))": 4,
        }
        for pattern, expected in cases.items():
            with self.subTest(pattern=pattern):
                self.assertEqual(
                    OWNER_REGEX_CHECKS.regex_capture_group_count(pattern), expected
                )

    def test_extract_distinguishes_no_match_from_an_unentered_group(self):
        self.assertIs(
            OWNER_REGEX_CHECKS.regex_extract("^a(b)?$", "z", 0),
            OWNER_REGEX_CHECKS.REGEX_NO_MATCH,
        )
        self.assertIsNone(OWNER_REGEX_CHECKS.regex_extract("^a(b)?$", "a", 1))
        self.assertEqual(OWNER_REGEX_CHECKS.regex_extract("^a(b)?$", "ab", 1), "b")

    def test_extract_returns_the_empty_string_for_an_empty_match(self):
        self.assertEqual(OWNER_REGEX_CHECKS.regex_extract("x*", "y", 0), "")

    def test_extract_reads_a_group_by_scalar_not_by_byte(self):
        subject = "x" + chr(0x1D400) + "-tail"
        self.assertEqual(
            OWNER_REGEX_CHECKS.regex_extract(r"(\u{1D400})-(\w+)", subject, 2), "tail"
        )

    def test_extract_rejects_a_group_the_pattern_does_not_declare(self):
        for group in (2, -1):
            with self.subTest(group=group):
                with self.assertRaises(OWNER_REGEX_CHECKS.RegexGroupOutOfRange):
                    OWNER_REGEX_CHECKS.regex_extract("^A-([0-9]+)$", "A-1", group)

    # -- the validation surface --------------------------------------------

    def test_both_conditions_are_registered_to_this_rule(self):
        registry = OWNER_REPOSITORY_MANIFESTS.VALIDATION_CONDITION_REGISTRY
        for condition, required in (
            # The binding's wording is not a portable fact, so a fixture
            # states the pattern and the diagnostic adds the reason.
            ("invalid_regex", {"pattern"}),
            ("regex_group_out_of_range", {"group", "group_count", "pattern"}),
        ):
            with self.subTest(condition=condition):
                registration = registry.get(("R022", condition))
                self.assertIsNotNone(registration)
                self.assertEqual(registration["required_context"], required)
                self.assertEqual(registration["allowed_phases"], {"validation"})

    def test_matches_pattern_the_contract_rejects_fails_validation(self):
        errors = OWNER_REGEX_CHECKS.regex_pattern_errors(
            "(?P<name>a)",
            "spec.columns.SEX.verifications[0].matches.pattern",
            full_match=True,
        )
        self.assertEqual(len(errors), 1)
        self.assertEqual(errors[0].condition, "invalid_regex")
        self.assertEqual(
            errors[0].path,
            "spec.columns.SEX.verifications[0].matches.pattern",
        )
        self.assertEqual(errors[0].context["pattern"], "(?P<name>a)")
        self.assertIn("reason", errors[0].context)

    def test_matches_pattern_the_contract_accepts_validates(self):
        errors = OWNER_REGEX_CHECKS.regex_pattern_errors(
            "(?<name>a)",
            "spec.columns.SEX.verifications[0].matches.pattern",
            full_match=True,
        )
        self.assertEqual(errors, [])

    def test_str_extract_group_outside_the_pattern_fails_validation(self):
        errors = OWNER_REGEX_CHECKS.regex_group_errors(
            "str_extract",
            {
                "source": "USUBJID",
                "pattern": "^A-([0-9]+)$",
                "group": 2,
            },
            "spec.columns.SITEID.derivation.str_extract",
        )
        self.assertEqual(len(errors), 1)
        self.assertEqual(errors[0].condition, "regex_group_out_of_range")
        self.assertEqual(
            errors[0].path,
            "spec.columns.SITEID.derivation.str_extract.group",
        )
        self.assertEqual(
            errors[0].context,
            {"group": 2, "group_count": 1, "pattern": "^A-([0-9]+)$"},
        )

    def test_str_extract_group_inside_the_pattern_validates(self):
        errors = OWNER_REGEX_CHECKS.regex_group_errors(
            "str_extract",
            {
                "source": "USUBJID",
                "pattern": "^A-([0-9]+)$",
                "group": 1,
            },
            "spec.columns.SITEID.derivation.str_extract",
        )
        self.assertEqual(errors, [])

    def test_a_parenthesis_in_a_class_declares_no_group(self):
        errors = OWNER_REGEX_CHECKS.regex_group_errors(
            "str_extract",
            {
                "source": "USUBJID",
                "pattern": "^[(]x$",
                "group": 1,
            },
            "spec.columns.SITEID.derivation.str_extract",
        )
        self.assertEqual(len(errors), 1)
        self.assertEqual(errors[0].condition, "regex_group_out_of_range")
        self.assertEqual(errors[0].context["group_count"], 0)

    def test_descriptor_pattern_the_contract_rejects_fails_validation(self):
        errors = OWNER_REPOSITORY_SCHEMA.check_descriptor(
            {"type": "str", "pattern": "(?P<name>a)"}, False, "schema.name"
        )
        self.assertEqual(len(errors), 1)
        self.assertEqual(errors[0].condition, "invalid_regex")

    def test_descriptor_pattern_constraint_uses_the_portable_binding(self):
        env = {
            "classes": {},
            "aliases": {"code": {"type": "str", "pattern": "^a$"}},
            "registries": {},
        }
        self.assertEqual(
            OWNER_REPOSITORY_SCHEMA.validate_type("a", ["code"], env, "spec.code"), []
        )
        # Python re.match would accept the trailing line feed here.
        self.assertTrue(
            OWNER_REPOSITORY_SCHEMA.validate_type("a\n", ["code"], env, "spec.code")
        )

    # -- the shared fixtures ------------------------------------------------

    def test_shared_fixtures_replay_against_the_portable_binding(self):
        self.assertEqual(OWNER_REGEX_CHECKS.validate_regex_conformance(self.root), [])

    def test_replay_reports_an_outcome_that_drifted_from_the_binding(self):
        source = self.root / "yaml" / "conformance" / "regex.yaml"
        original = source.read_text()
        drifted = original.replace(
            "    pattern: '^\\d$'\n    subject: \"5\"\n    schema_pattern: true",
            "    pattern: '^\\d$'\n    subject: \"5\"\n    schema_pattern: false",
            1,
        )
        self.assertNotEqual(drifted, original)
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            target = root / "yaml" / "conformance"
            target.mkdir(parents=True)
            (target / "regex.yaml").write_text(drifted)
            errors = OWNER_REGEX_CHECKS.validate_regex_conformance(root)
        self.assertEqual(len(errors), 1)
        self.assertIn("schema_pattern records False", errors[0])

    def test_replay_reports_a_pattern_the_binding_does_not_reject(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            target = root / "yaml" / "conformance"
            target.mkdir(parents=True)
            (target / "regex.yaml").write_text(
                'schema_version: "1.0"\n'
                "contract: regex\n"
                'contract_version: "2.0.0"\n'
                "cases:\n"
                "  - id: accepted-pattern-recorded-as-invalid\n"
                "    covers: [unsupported]\n"
                "    pattern: 'a'\n"
                "    invalid: true\n"
            )
            errors = OWNER_REGEX_CHECKS.validate_regex_conformance(root)
        self.assertTrue(
            any("the portable binding accepts pattern" in e for e in errors),
            errors,
        )

    def test_replay_requires_a_case_for_every_named_category(self):
        self.assertTrue(OWNER_REGEX_CHECKS.REGEX_FIXTURE_REQUIRED_COVERS)
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            target = root / "yaml" / "conformance"
            target.mkdir(parents=True)
            (target / "regex.yaml").write_text(
                'schema_version: "1.0"\n'
                "contract: regex\n"
                'contract_version: "2.0.0"\n'
                "cases:\n"
                "  - id: only-an-anchor-case\n"
                "    covers: [anchors]\n"
                "    pattern: '^a$'\n"
                '    subject: "a"\n'
                "    schema_pattern: true\n"
                "    matches: true\n"
                '    str_extract: {group: 0, value: "a"}\n'
            )
            errors = OWNER_REGEX_CHECKS.validate_regex_conformance(root)
        self.assertEqual(len(errors), 1)
        self.assertIn("no case covers", errors[0])

    def test_missing_fixture_file_fails_validation(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            errors = OWNER_REGEX_CHECKS.validate_regex_conformance(Path(temp_dir))
        self.assertEqual(len(errors), 1)
        self.assertIn("missing R022", errors[0])


class TestClosedGrammarContracts(unittest.TestCase):
    """One machine-readable grammar per closed language, read by everyone."""

    @classmethod
    def setUpClass(cls):
        cls.root = ROOT

    def contract_root(self, temp_dir, **replacements):
        """Copy the grammars and rules into a root, applying replacements."""
        root = Path(temp_dir)
        shutil.copytree(self.root / "yaml" / "grammar", root / "yaml" / "grammar")
        shutil.copytree(self.root / "rules", root / "rules")
        for relative, (old, new) in replacements.items():
            path = root / relative
            text = path.read_text()
            self.assertIn(old, text)
            path.write_text(text.replace(old, new, 1))
        return root

    # -- the grammar is one source -----------------------------------------

    def test_every_contract_replays_against_this_validator(self):
        self.assertEqual(OWNER_GRAMMAR_CHECKS.validate_grammar_contracts(self.root), [])

    def test_every_rule_that_owns_a_grammar_has_a_contract(self):
        for contract, rule_id in OWNER_GRAMMAR_CHECKS.GRAMMAR_CONTRACTS.items():
            with self.subTest(contract=contract):
                path = self.root / "yaml" / "grammar" / f"{contract}.yaml"
                self.assertTrue(path.is_file())
                document = yaml.safe_load(path.read_text())
                self.assertEqual(document["rule"], rule_id)

    def test_a_continuation_aligns_under_its_definition_or_its_bar(self):
        rendered = OWNER_GRAMMAR_CHECKS.render_grammar_block(
            [
                {"name": "comparison", "definition": "a b\n| c d"},
                {"name": "null_test", "definition": "e\nf"},
            ]
        )
        self.assertEqual(
            rendered,
            "comparison := a b\n            | c d\nnull_test  := e\n              f",
        )

    def test_a_rule_block_that_drifts_from_its_grammar_is_reported(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.contract_root(
                temp_dir,
                **{
                    "rules/operations/text.md": (
                        'placeholder := "{" variable "}"',
                        'placeholder := "{" variable "}" | variable',
                    )
                },
            )
            errors = OWNER_GRAMMAR_CHECKS.validate_grammar_contract(
                root, "string-template"
            )
        self.assertEqual(len(errors), 1)
        self.assertIn("operations/text.md", errors[0])
        self.assertIn("the grammar block is not the one", errors[0])

    def test_a_shape_that_drifts_from_this_parser_is_reported(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.contract_root(
                temp_dir,
                **{
                    "yaml/grammar/numeric.yaml": (
                        "shape: (+ (id A) (* (id B) (id C)))",
                        "shape: (* (+ (id A) (id B)) (id C))",
                    )
                },
            )
            errors = OWNER_GRAMMAR_CHECKS.validate_grammar_contract(root, "numeric")
        self.assertEqual(len(errors), 1)
        self.assertIn("multiplication-binds-tighter-than-addition", errors[0])
        self.assertIn("this validator parses", errors[0])

    def test_an_identifier_set_that_drifts_from_this_parser_is_reported(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.contract_root(
                temp_dir,
                **{
                    "yaml/grammar/predicate.yaml": (
                        "identifiers: [EX.EXDOSE]",
                        "identifiers: [EXDOSE]",
                    )
                },
            )
            errors = OWNER_GRAMMAR_CHECKS.validate_grammar_contract(root, "predicate")
        self.assertEqual(len(errors), 1)
        self.assertIn("this validator collects", errors[0])

    def test_a_text_this_parser_accepts_cannot_be_recorded_as_rejected(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.contract_root(
                temp_dir,
                **{
                    "yaml/grammar/aggregate.yaml": (
                        "  - id: a-reduction-reads-one-relation-field\n"
                        "    covers: [reduction]\n"
                        '    text: "SUM(EX.EXDOSE)"\n'
                        "    parse: accept\n"
                        "    identifiers: [EX.EXDOSE]\n"
                        "    shape: (reduce SUM (id EX.EXDOSE))\n",
                        "  - id: a-reduction-reads-one-relation-field\n"
                        "    covers: [reduction]\n"
                        '    text: "SUM(EX.EXDOSE)"\n'
                        "    parse: reject\n"
                        "    condition: invalid_aggregate_expression\n",
                    )
                },
            )
            errors = OWNER_GRAMMAR_CHECKS.validate_grammar_contract(root, "aggregate")
        self.assertEqual(len(errors), 1)
        self.assertIn("records as rejected", errors[0])

    def test_a_condition_that_drifts_from_this_parser_is_reported(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.contract_root(
                temp_dir,
                **{
                    "yaml/grammar/numeric.yaml": (
                        '    text: "ROUND(A, 2)"\n'
                        "    parse: reject\n"
                        "    condition: prohibited_function\n",
                        '    text: "ROUND(A, 2)"\n'
                        "    parse: reject\n"
                        "    condition: prohibited_construct\n",
                    )
                },
            )
            errors = OWNER_GRAMMAR_CHECKS.validate_grammar_contract(root, "numeric")
        self.assertEqual(len(errors), 1)
        self.assertIn("fails with 'prohibited_function'", errors[0])

    # -- the vocabulary is one source --------------------------------------

    def test_a_vocabulary_that_drifts_from_this_parser_is_reported(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.contract_root(
                temp_dir,
                **{
                    "yaml/grammar/numeric.yaml": (
                        "SQRT: {min_arguments: 1, max_arguments: 1}",
                        "SQRT: {min_arguments: 1, max_arguments: 2}",
                    )
                },
            )
            errors = OWNER_GRAMMAR_CHECKS.validate_grammar_contract(root, "numeric")
        self.assertEqual(len(errors), 1)
        self.assertIn("vocabulary.function does not name", errors[0])

    def test_a_reserved_list_that_drifts_from_this_parser_is_reported(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.contract_root(
                temp_dir,
                **{"yaml/grammar/predicate.yaml": ('  - "ESCAPE"\n', "")},
            )
            errors = OWNER_GRAMMAR_CHECKS.validate_grammar_contract(root, "predicate")
        self.assertEqual(len(errors), 1)
        self.assertIn("does not name the names this validator", errors[0])

    def test_a_reducer_list_that_drifts_from_this_parser_is_reported(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.contract_root(
                temp_dir,
                **{
                    "yaml/grammar/aggregate.yaml": (
                        "    ONLY: {argument: [expr]}",
                        "    AVG: {argument: [expr]}",
                    )
                },
            )
            errors = OWNER_GRAMMAR_CHECKS.validate_grammar_contract(root, "aggregate")
        self.assertTrue(
            any("does not name the reducers" in error for error in errors),
            errors,
        )

    # -- the file is machine-checkable -------------------------------------

    def test_a_production_may_not_name_an_undefined_symbol(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.contract_root(
                temp_dir,
                **{
                    "yaml/grammar/string-template.yaml": (
                        "imports:\n  variable: schema\n",
                        "imports: {}\n",
                    )
                },
            )
            errors = OWNER_GRAMMAR_CHECKS.validate_grammar_contract(
                root, "string-template"
            )
        self.assertEqual(len(errors), 1)
        self.assertIn("placeholder: undefined symbols ['variable']", errors[0])

    def test_an_import_must_name_a_contract_that_defines_it(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.contract_root(
                temp_dir,
                **{
                    "yaml/grammar/numeric.yaml": (
                        "imports:\n  name: predicate\n",
                        "imports:\n  name: string-template\n",
                    )
                },
            )
            errors = OWNER_GRAMMAR_CHECKS.validate_grammar_contract(root, "numeric")
        self.assertEqual(len(errors), 1)
        self.assertIn("defines no 'name' to import", errors[0])

    def test_replay_requires_a_case_for_every_named_category(self):
        self.assertIn(
            "rejection", OWNER_GRAMMAR_CHECKS.GRAMMAR_COVERS["string-template"]
        )
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.contract_root(temp_dir)
            source = root / "yaml" / "grammar" / "string-template.yaml"
            head, separator, cases = source.read_text().partition("cases:\n")
            self.assertTrue(separator)
            kept = [
                block
                for block in cases.split("  - id: ")[1:]
                if "parse: reject" not in block
            ]
            source.write_text(
                head + separator + "".join(f"  - id: {block}" for block in kept)
            )
            errors = OWNER_GRAMMAR_CHECKS.validate_grammar_contract(
                root, "string-template"
            )
        self.assertEqual(len(errors), 1)
        self.assertIn("no case covers ['rejection']", errors[0])

    def test_a_missing_grammar_fails_validation(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            errors = OWNER_GRAMMAR_CHECKS.validate_grammar_contracts(Path(temp_dir))
        self.assertEqual(len(errors), len(OWNER_GRAMMAR_CHECKS.GRAMMAR_CONTRACTS))
        for error in errors:
            self.assertIn("missing machine-readable grammar", error)

    # -- the R parser reads the same files ----------------------------------

    def test_the_r_runner_reads_the_same_contracts(self):
        stager = Path(stage_grammar_inputs.__file__)
        text = stager.read_text()
        for contract, rule_id in OWNER_GRAMMAR_CHECKS.GRAMMAR_CONTRACTS.items():
            with self.subTest(contract=contract):
                self.assertIn(contract, text)
                self.assertIn(rule_id, text)
        self.assertIn('"yaml/grammar"', text)
        runner = self.root / "R" / "yamaa" / "tests" / "public_grammar.R"
        self.assertIn('"grammar-original"', runner.read_text())
        self.assertIn("yamaa_domain(", runner.read_text())


class TestDeclaredValidationErrors(unittest.TestCase):
    """A fixture must fail for the condition it declares."""

    def check(self, condition, reported):
        return OWNER_REPOSITORY_MANIFESTS.check_declared_validation_error(
            {"condition": condition, "spec_paths": ["input.REF.path"]},
            reported,
            "example/spec.yaml",
            "benchmark/example/expected/error.yaml",
        )

    def test_accepts_a_reported_condition(self):
        self.assertEqual(
            self.check(
                "resource_path_missing",
                ["resource_path_missing: 'input/ref.csv' does not exist"],
            ),
            [],
        )

    def test_rejects_a_condition_the_validator_did_not_report(self):
        errors = self.check(
            "resource_path_missing",
            ["resource_path_symlink: 'input/ref.csv' passes through a link"],
        )

        self.assertEqual(len(errors), 1)
        self.assertIn("'resource_path_missing' was not reported", errors[0])
        self.assertIn("['resource_path_symlink']", errors[0])

    def test_rejects_a_fixture_that_no_longer_fails(self):
        errors = self.check("resource_path_missing", [])

        self.assertEqual(len(errors), 1)
        self.assertIn("was not reported", errors[0])

    def test_ignores_a_condition_the_validator_does_not_decide(self):
        self.assertEqual(self.check("aggregate_over_scalar_source", []), [])


class TestIntermediateDerivedFilterValidation(unittest.TestCase):
    """REQ-1185: an intermediate's filter resolves dataset-qualified derived names."""

    def validate(self, filt, extra_derivations=None, column_predicate=None):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            input_dir = root / "input"
            input_dir.mkdir()
            (input_dir / "dm.csv").write_text(
                "STUDYID,USUBJID\nYAMAA-01,P01\n", encoding="utf-8"
            )
            (input_dir / "ds.csv").write_text(
                "STUDYID,USUBJID,DSSEQ,DSSTDY\nYAMAA-01,P01,1,10\n",
                encoding="utf-8",
            )
            derivations = {
                "EOT_FALLBACK": {"compute": {"expr": "DSSTDY + DSSEQ / 1000"}},
            }
            derivations.update(extra_derivations or {})
            spec = {
                "schema_version": "1.0",
                "domain": "ADSL",
                "keys": ["STUDYID", "USUBJID"],
                "input": {
                    "DM": "input/dm.csv",
                    "DS": {
                        "path": "input/ds.csv",
                        "types": {"DSSEQ": "int", "DSSTDY": "int"},
                    },
                },
                "base": "DM",
                "columns": [
                    {"name": "STUDYID", "type": "str", "derivation": "DM.STUDYID"},
                    {"name": "USUBJID", "type": "str", "derivation": "DM.USUBJID"},
                ],
                "intermediates": [
                    {
                        "id": "EOT",
                        "dataset": "DS",
                        "derivations": derivations,
                        "filter": filt,
                    }
                ],
            }
            if column_predicate is not None:
                spec["columns"] += [
                    {
                        "name": "FLAG",
                        "type": "str",
                        "derivation": {
                            "case": [
                                {"when": column_predicate, "then": {"literal": "Y"}},
                                {"otherwise": {"literal": "N"}},
                            ]
                        },
                    }
                ]
            spec_path = root / "spec.yaml"
            return CONTROLS.document(spec, "spec.yaml", spec_path, None)

    def conditions(self, errors):
        return [getattr(error, "condition", "?") for error in errors]

    def test_derived_name_resolves_in_filter(self):
        self.assertEqual(
            self.conditions(self.validate("DS.EOT_FALLBACK IS NOT NULL")), []
        )

    def test_unknown_derived_name_still_fails(self):
        errors = self.validate("DS.NOPE IS NOT NULL")
        self.assertEqual(self.conditions(errors), ["unknown_field"])

    def test_derived_name_keeps_its_inferred_type(self):
        errors = self.validate("DS.EOT_FALLBACK = 'x'")
        self.assertEqual(self.conditions(errors), ["incompatible_input_type"])
        self.assertEqual(self.conditions(self.validate("DS.EOT_FALLBACK > 1.5")), [])

    def test_untypable_derivation_is_known_but_unchecked(self):
        derivations = {
            "LABEL": {
                "case": [
                    {"when": "DSSEQ = 1", "then": {"literal": "first"}},
                    {"otherwise": {"literal": "other"}},
                ]
            }
        }
        self.assertEqual(
            self.conditions(self.validate("DS.LABEL = 'first'", derivations)),
            [],
        )

    def test_downstream_predicate_reads_intermediate_derived_name(self):
        self.assertEqual(
            self.conditions(
                self.validate(
                    "DS.EOT_FALLBACK IS NOT NULL",
                    column_predicate="EOT.EOT_FALLBACK > 1.5",
                )
            ),
            [],
        )

    def test_downstream_predicate_checks_derived_type(self):
        self.assertEqual(
            self.conditions(
                self.validate(
                    "DS.EOT_FALLBACK IS NOT NULL",
                    column_predicate="EOT.EOT_FALLBACK = 'x'",
                )
            ),
            ["incompatible_input_type"],
        )


class TestDatasetPathExamples(unittest.TestCase):
    """Current path conditions agree with fixtures; retired ones stay history."""

    def setUp(self):
        self.root = ROOT
        self.env, schema_errors = OWNER_REPOSITORY_SCHEMA.build_schema_env(self.root)
        self.assertEqual(schema_errors, [])

    def test_each_current_example_reports_its_declared_condition(self):
        examples = sorted((self.root / "benchmarks").glob("negative-path-*"))
        self.assertEqual(len(examples), 6)

        for example in examples:
            if example.name in OWNER_REPOSITORY_STRUCTURE.HISTORICAL_RESOURCE_FIXTURES:
                continue
            with self.subTest(example=example.name):
                spec_path = example / "spec.yaml"
                spec = yaml.load(
                    spec_path.read_text(encoding="utf-8"),
                    Loader=OWNER_REPOSITORY_YAML.UniqueKeyLoader,
                )
                declared = yaml.load(
                    (example / "expected" / "error.yaml").read_text(encoding="utf-8"),
                    Loader=OWNER_REPOSITORY_YAML.UniqueKeyLoader,
                )
                reported = OWNER_REPOSITORY_SCHEMA.validate_spec_document(
                    spec, f"{example.name}/spec.yaml", spec_path, self.env
                )

                self.assertEqual(declared["phase"], "validation")
                self.assertEqual(len(reported), 1)
                self.assertIn(
                    f"input.LBREF.path: {declared['condition']}: ",
                    reported[0],
                )
                self.assertIn(repr(declared["context"]["path"]), reported[0])

    def test_historical_paths_use_declaring_location_without_containment(self):
        # REQ-0780/0784: these absolute and parent-relative spellings are
        # allowed. Their files are absent; their committed retired-policy
        # expectations remain historical rather than current acceptance.
        for name in sorted(OWNER_REPOSITORY_STRUCTURE.HISTORICAL_RESOURCE_FIXTURES):
            with self.subTest(example=name):
                spec_path = self.root / "benchmarks" / name / "spec.yaml"
                spec = yaml.load(
                    spec_path.read_text(encoding="utf-8"),
                    Loader=OWNER_REPOSITORY_YAML.UniqueKeyLoader,
                )
                source = spec["input"]["LBREF"]
                written = source if isinstance(source, str) else source["path"]
                self.assertEqual(
                    SOURCE_CONTROLS.resource(written, spec_path.parent)[1],
                    "resource_path_missing",
                )
                reported = OWNER_REPOSITORY_SCHEMA.validate_spec_document(
                    spec, f"{name}/spec.yaml", spec_path, self.env
                )
                self.assertEqual(len(reported), 1)
                self.assertIn("input.LBREF.path: resource_path_missing: ", reported[0])
                self.assertIn(repr(written), reported[0])


class TestReferenceDependencyBoundary(unittest.TestCase):
    """#2002 rejects reference fallback across every retained tool module."""

    @staticmethod
    def forbidden_imports(source):
        forbidden = []
        for node in ast.walk(ast.parse(source)):
            if isinstance(node, ast.Import):
                names = [alias.name for alias in node.names]
            elif isinstance(node, ast.ImportFrom):
                names = [node.module or ""]
                if node.module == "yamaa":
                    names = ["yamaa." + alias.name for alias in node.names]
            else:
                continue
            forbidden.extend(
                name
                for name in names
                if (
                    name.startswith("yamaa_reference")
                    or (
                        name.startswith("yamaa.")
                        and name not in {"yamaa._native", "yamaa.check", "yamaa.domain"}
                    )
                )
            )
        return forbidden

    def test_validator_has_no_reference_import_or_source_loader(self):
        source = TOOL_PATH.read_text(encoding="utf-8")
        self.assertEqual(self.forbidden_imports(source), [])
        calls = [
            node.func
            for node in ast.walk(ast.parse(source))
            if isinstance(node, ast.Call)
        ]
        self.assertFalse(
            any(
                isinstance(call, ast.Attribute)
                and call.attr == "spec_from_file_location"
                for call in calls
            ),
            "the validator cannot load retired reference source files",
        )
        owner_source = TOOL_PATH.with_name("repository_schema.py").read_text(
            encoding="utf-8"
        )
        self.assertEqual(self.forbidden_imports(owner_source), [])
        entry = next(
            node
            for node in ast.parse(owner_source).body
            if isinstance(node, ast.FunctionDef)
            and node.name == "validate_spec_document"
        )
        semantic = [
            node.func.id
            for node in ast.walk(entry)
            if isinstance(node, ast.Call)
            and isinstance(node.func, ast.Name)
            and (
                node.func.id.startswith("validate_spec_")
                or node.func.id
                in {
                    "validate_source_ordinal",
                    "validate_static_semantics",
                    "validate_numeric_expression_ast",
                    "validate_predicate_ast",
                }
            )
        ]
        self.assertEqual(
            semantic, [], "production admission cannot fall back to partial controls"
        )
        native_source = TOOL_PATH.with_name("_native_specification.py").read_text()
        self.assertNotIn("_check_project_metadata", native_source)
        self.assertNotIn(".build(", native_source)
        self.assertNotIn(".save(", native_source)

    def test_csv_fixture_admission_uses_only_the_retained_public_native_surface(self):
        source = TOOL_PATH.with_name("_native_csv.py").read_text(encoding="utf-8")
        self.assertEqual(self.forbidden_imports(source), [])
        self.assertNotIn("yamaa.io.csv", source)
        self.assertNotIn("yamaa.models", source)
        self.assertNotIn("spec_from_file_location", source)

    def test_regex_fixture_admission_uses_only_public_native_specifications(self):
        source = TOOL_PATH.with_name("_native_regex.py").read_text(encoding="utf-8")
        self.assertEqual(self.forbidden_imports(source), [])
        self.assertNotIn(".save(", source)
        self.assertNotIn("yamaa.regex", source)

    def test_guard_rejects_direct_aliased_and_from_package_imports(self):
        for source in (
            "import yamaa.expressions.predicates",
            "import yamaa.models as model",
            "from yamaa.io.csv import scan_records as scan",
            "from yamaa import schema",
            "from yamaa.odm.items import parse_odm_read",
            "import yamaa_reference as evaluator",
        ):
            with self.subTest(source=source):
                self.assertTrue(self.forbidden_imports(source))

    def test_public_check_and_native_metadata_imports_are_allowed(self):
        self.assertEqual(
            self.forbidden_imports(
                "import yamaa\n"
                "from yamaa import check as shared_check\n"
                "from yamaa._native import _prepare_file_project\n"
            ),
            [],
        )

    def test_boundary_has_no_dependency_back_on_the_validator(self):
        self.assertFalse(TOOL_PATH.with_name("_reference_checks.py").exists())
        for path in TOOL_PATH.parent.glob("_native_*.py"):
            source = path.read_text(encoding="utf-8")
            with self.subTest(module=path.name):
                self.assertEqual(self.forbidden_imports(source), [])
                imports = [
                    node
                    for node in ast.walk(ast.parse(source))
                    if isinstance(node, (ast.Import, ast.ImportFrom))
                ]
                names = [
                    name
                    for node in imports
                    for name in (
                        [alias.name for alias in node.names]
                        if isinstance(node, ast.Import)
                        else [node.module]
                    )
                ]
                self.assertNotIn("validate_repository", names)
                self.assertNotIn("spec_from_file_location", source)
                self.assertNotIn("yamaa_reference", source)


class TestValidatorCLI(unittest.TestCase):
    def setUp(self):
        self.test_dir = tempfile.TemporaryDirectory()
        self.root_dir = Path(self.test_dir.name)
        # Every repository carries R022's shared fixtures and the closed
        # grammars, which are read against their rules, so a synthetic root
        # that expects a clean run needs all of them too.
        repository = ROOT
        conformance = self.root_dir / "yaml" / "conformance"
        conformance.mkdir(parents=True)
        shutil.copy(
            repository / "yaml" / "conformance" / "regex.yaml",
            conformance / "regex.yaml",
        )
        shutil.copytree(
            repository / "yaml" / "grammar", self.root_dir / "yaml" / "grammar"
        )
        shutil.copytree(repository / "rules", self.root_dir / "rules")

    def tearDown(self):
        self.test_dir.cleanup()

    def test_schema_fields_from_expands_and_rejects_duplicates(self):
        schema_path = self.root_dir / "yaml" / "schema.yaml"
        schema_path.write_text(
            'version: "1.0"\n'
            "shared_fields:\n  - filter: {type: str}\n"
            "root_class:\n  - id: {type: str}\n"
            "  - fields_from: shared_fields\n",
            encoding="ascii",
        )
        env, errors = OWNER_REPOSITORY_SCHEMA.build_schema_env(self.root_dir)
        self.assertEqual(errors, [])
        self.assertEqual(
            set(OWNER_REPOSITORY_SCHEMA.schema_class_fields(env, "root_class")),
            {"id", "filter"},
        )

        schema_path.write_text(
            schema_path.read_text(encoding="ascii") + "  - filter: {type: str}\n",
            encoding="ascii",
        )
        _, errors = OWNER_REPOSITORY_SCHEMA.build_schema_env(self.root_dir)
        self.assertTrue(any("duplicate class field" in error for error in errors))

        for target, expected in (
            ("missing_fields", "unknown fields_from class"),
            ("root_class", "class fields_from cycle"),
        ):
            with self.subTest(target=target):
                schema_path.write_text(
                    f'version: "1.0"\nroot_class:\n  - fields_from: {target}\n',
                    encoding="ascii",
                )
                _, errors = OWNER_REPOSITORY_SCHEMA.build_schema_env(self.root_dir)
                self.assertTrue(any(expected in error for error in errors))

    def test_cli_help(self):
        result = subprocess.run(
            [sys.executable, "-m", "tools.validation.validate_repository", "--help"],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0)
        self.assertIn("--root", result.stdout)
        self.assertIn("--warnings-as-errors", result.stdout)

    def test_cli_missing_schema_fails(self):
        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.validation.validate_repository",
                "--root",
                str(self.root_dir),
            ],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("schema.yaml", result.stdout)

    def test_yaml_duplicate_keys(self):
        yaml_dir = self.root_dir / "yaml"
        yaml_dir.mkdir(exist_ok=True)
        bad_yaml = yaml_dir / "bad.yaml"
        bad_yaml.write_text("a: 1\na: 2\n")

        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.validation.validate_repository",
                "--root",
                str(self.root_dir),
            ],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("ERROR", result.stdout)
        self.assertIn("bad.yaml", result.stdout)
        self.assertIn("line 2", result.stdout.lower() + result.stderr.lower())

    def test_yaml_syntax_error(self):
        yaml_dir = self.root_dir / "yaml"
        yaml_dir.mkdir(exist_ok=True)
        bad_yaml = yaml_dir / "syntax.yaml"
        bad_yaml.write_text("a: 1\n  b: 2\n")

        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.validation.validate_repository",
                "--root",
                str(self.root_dir),
            ],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("ERROR", result.stdout)
        self.assertIn("syntax.yaml", result.stdout)

    def test_schema_include_cycle(self):
        yaml_dir = self.root_dir / "yaml"
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / "schema.yaml").write_text(
            'version: "1.0"\nincludes: ["schema_cycle.yaml"]\nroot_class:\n  - f1: {type: str}\n'
        )
        (yaml_dir / "schema_cycle.yaml").write_text(
            'version: "1.0"\nincludes: ["schema.yaml"]\n'
        )

        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.validation.validate_repository",
                "--root",
                str(self.root_dir),
            ],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("ERROR", result.stdout)
        self.assertIn("cycle", result.stdout.lower())

    def test_schema_shared_include_is_not_a_cycle(self):
        yaml_dir = self.root_dir / "yaml"
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / "schema.yaml").write_text(
            'version: "1.0"\nincludes: ["schema_left.yaml", "schema_right.yaml"]\n'
            "root_class:\n  - f1: {type: str}\n"
        )
        (yaml_dir / "schema_left.yaml").write_text(
            'version: "1.0"\nincludes: ["schema_shared.yaml"]\n'
        )
        (yaml_dir / "schema_right.yaml").write_text(
            'version: "1.0"\nincludes: ["schema_shared.yaml"]\n'
        )
        (yaml_dir / "schema_shared.yaml").write_text('version: "1.0"\n')

        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.validation.validate_repository",
                "--root",
                str(self.root_dir),
            ],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_schema_include_cannot_escape_yaml_directory(self):
        yaml_dir = self.root_dir / "yaml"
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / "schema.yaml").write_text(
            'version: "1.0"\nincludes: ["../outside.yaml"]\n'
            "root_class:\n  - f1: {type: str}\n"
        )
        (self.root_dir / "outside.yaml").write_text('version: "1.0"\n')

        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.validation.validate_repository",
                "--root",
                str(self.root_dir),
            ],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("inside the yaml directory", result.stdout.lower())

    def test_schema_include_rejects_symlink_inside_yaml_directory(self):
        yaml_dir = self.root_dir / "yaml"
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / "schema.yaml").write_text(
            'version: "1.0"\nincludes: ["schema_link.yaml"]\n'
            "root_class:\n  - f1: {type: str}\n"
        )
        (yaml_dir / "schema_real.yaml").write_text('version: "1.0"\n')
        (yaml_dir / "schema_link.yaml").symlink_to("schema_real.yaml")

        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.validation.validate_repository",
                "--root",
                str(self.root_dir),
            ],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("symlink", result.stdout.lower())

    def test_schema_include_requires_mapping_root(self):
        yaml_dir = self.root_dir / "yaml"
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / "schema.yaml").write_text(
            'version: "1.0"\nincludes: ["schema_extra.yaml"]\n'
            "root_class:\n  - f1: {type: str}\n"
        )
        (yaml_dir / "schema_extra.yaml").write_text("- not\n- a\n- mapping\n")

        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.validation.validate_repository",
                "--root",
                str(self.root_dir),
            ],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("mapping", result.stdout.lower())
        self.assertIn("schema_extra.yaml", result.stdout)

    def test_schema_include_name_must_follow_convention(self):
        yaml_dir = self.root_dir / "yaml"
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / "schema.yaml").write_text(
            'version: "1.0"\nincludes: ["other.yaml"]\n'
            "root_class:\n  - f1: {type: str}\n"
        )
        (yaml_dir / "other.yaml").write_text('version: "1.0"\n')

        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.validation.validate_repository",
                "--root",
                str(self.root_dir),
            ],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("schema_[a-z0-9_]+.yaml", result.stdout)

    def test_duplicate_schema_declaration_fails(self):
        yaml_dir = self.root_dir / "yaml"
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / "schema.yaml").write_text(
            'version: "1.0"\nincludes: ["schema_extra.yaml"]\n'
            "root_class:\n  - first: {type: str}\n"
        )
        (yaml_dir / "schema_extra.yaml").write_text(
            'version: "1.0"\nroot_class:\n  - second: {type: str}\n'
        )

        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.validation.validate_repository",
                "--root",
                str(self.root_dir),
            ],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("duplicate declaration", result.stdout)

    def test_invalid_descriptor_keyword_fails(self):
        yaml_dir = self.root_dir / "yaml"
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / "schema.yaml").write_text(
            'version: "1.0"\nroot_class:\n  - first: {type: str, not_a_keyword: true}\n'
        )

        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.validation.validate_repository",
                "--root",
                str(self.root_dir),
            ],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("not_a_keyword", result.stdout)

    def test_malformed_class_entry_fails_actionably(self):
        yaml_dir = self.root_dir / "yaml"
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / "schema.yaml").write_text(
            'version: "1.0"\nroot_class:\n  - invalid\n'
        )

        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.validation.validate_repository",
                "--root",
                str(self.root_dir),
            ],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("one-entry mapping", result.stdout)
        self.assertNotIn("Traceback", result.stderr)

    def test_schema_registry_reference_must_resolve(self):
        yaml_dir = self.root_dir / "yaml"
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / "schema.yaml").write_text(
            'version: "1.0"\nroot_class:\n  - op: {type: expression}\n'
            "expression: {registry: missing_registry}\n"
        )

        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.validation.validate_repository",
                "--root",
                str(self.root_dir),
            ],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("missing_registry", result.stdout)

    def test_schema_unknown_type(self):
        yaml_dir = self.root_dir / "yaml"
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / "schema.yaml").write_text(
            'version: "1.0"\nroot_class:\n  - f1: {type: unknown_type}\n'
        )

        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.validation.validate_repository",
                "--root",
                str(self.root_dir),
            ],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("ERROR", result.stdout)
        if "unknown_type" not in result.stdout:
            print("STDOUT:", result.stdout)
            print("STDERR:", result.stderr)
        self.assertIn("unknown_type", result.stdout)

    def test_schema_column_types_are_not_schema_builtins(self):
        yaml_dir = self.root_dir / "yaml"
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / "schema.yaml").write_text(
            'version: "1.0"\nroot_class:\n  - f1: {type: date}\n'
        )

        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.validation.validate_repository",
                "--root",
                str(self.root_dir),
            ],
            capture_output=True,
            text=True,
        )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unknown_type 'date'", result.stdout)

    def test_registry_name_is_not_a_schema_type(self):
        yaml_dir = self.root_dir / "yaml"
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / "schema.yaml").write_text(
            'version: "1.0"\n'
            "root_class:\n  - f1: {type: operations}\n"
            "operation: {registry: operations}\n"
            "operations:\n  one: {type: str}\n"
        )

        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.validation.validate_repository",
                "--root",
                str(self.root_dir),
            ],
            capture_output=True,
            text=True,
        )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unknown_type 'operations'", result.stdout)

    def test_schema_version_mismatch(self):
        yaml_dir = self.root_dir / "yaml"
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / "schema.yaml").write_text(
            'version: "1.0"\nincludes: ["schema_other.yaml"]\nroot_class:\n  - f1: {type: str}\n'
        )
        (yaml_dir / "schema_other.yaml").write_text('version: "2.0"\n')

        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.validation.validate_repository",
                "--root",
                str(self.root_dir),
            ],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("ERROR", result.stdout)
        self.assertIn("version", result.stdout.lower())

    def test_schema_bundle_version_is_not_hardcoded(self):
        yaml_dir = self.root_dir / "yaml"
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / "schema.yaml").write_text(
            'version: "2.0"\nincludes: ["schema_extra.yaml"]\n'
            "root_class:\n  - value: {type: str}\n"
        )
        (yaml_dir / "schema_extra.yaml").write_text('version: "2.0"\n')

        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.validation.validate_repository",
                "--root",
                str(self.root_dir),
            ],
            capture_output=True,
            text=True,
        )

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_documentation_missing_readme(self):
        ex_dir = self.root_dir / "benchmarks" / "bad-example"
        ex_dir.mkdir(parents=True, exist_ok=True)
        # missing everything
        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.validation.check_documentation",
                "--root",
                str(self.root_dir),
            ],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("README.md", result.stdout)

    def test_documentation_negative_missing_how_to_fix(self):
        ex_dir = self.root_dir / "benchmarks" / "negative-bad"
        ex_dir.mkdir(parents=True, exist_ok=True)
        (ex_dir / "README.md").write_text("# bad")
        (ex_dir / "spec.yaml").write_text("{}")
        (ex_dir / "input").mkdir()
        (ex_dir / "expected").mkdir()
        (ex_dir / "expected" / "error.yaml").write_text("{}")

        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.validation.check_documentation",
                "--root",
                str(self.root_dir),
            ],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("How to fix", result.stdout)

    def test_define_documents_are_validated_against_their_own_class(self):
        # R026 gives the study document its own entry point, so the
        # specification schema never reads it: nothing else would report a
        # field the document does not declare.
        root = ROOT

        self.assertEqual(
            OWNER_REPOSITORY_STRUCTURE.validate_examples_define_documents(root), []
        )

    def test_a_define_document_field_outside_its_class_is_reported(self):
        root = ROOT
        source = root / "benchmarks" / "sdtm-dm-metadata"
        with tempfile.TemporaryDirectory() as temp_dir:
            copy = Path(temp_dir)
            shutil.copytree(root / "yaml", copy / "yaml", dirs_exist_ok=True)
            shutil.copytree(
                root / "benchmarks", copy / "benchmarks", dirs_exist_ok=True
            )
            document = copy / "benchmarks" / source.name / "define.yaml"
            document.write_text(
                document.read_text().replace("\ndatasets:\n", "\ninput:\n", 1)
            )

            errors = OWNER_REPOSITORY_STRUCTURE.validate_examples_define_documents(copy)

        message = "\n".join(errors)
        self.assertIn("unknown field 'input' for class define_class", message)
        self.assertIn("missing required field 'datasets'", message)

    def test_example_layout_positive_has_error(self):
        ex_dir = self.root_dir / "benchmarks" / "positive-bad"
        ex_dir.mkdir(parents=True, exist_ok=True)
        (ex_dir / "README.md").write_text("# bad")
        (ex_dir / "spec.yaml").write_text("{}")
        (ex_dir / "input").mkdir()
        (ex_dir / "expected").mkdir()
        (ex_dir / "expected" / "error.yaml").write_text("{}")

        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.validation.validate_repository",
                "--root",
                str(self.root_dir),
            ],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("error.yaml", result.stdout)

    def test_example_index_stale(self):
        ex_dir = self.root_dir / "benchmarks"
        ex_dir.mkdir(parents=True, exist_ok=True)
        (ex_dir / "README.md").write_text(
            "# Index\n\n| [`stale`](stale/) | stale desc |\n"
        )

        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.validation.check_documentation",
                "--root",
                str(self.root_dir),
            ],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("stale", result.stdout)

    def test_example_index_link_must_match_directory(self):
        examples_dir = self.root_dir / "benchmarks"
        ex_dir = examples_dir / "good"
        ex_dir.mkdir(parents=True)
        (ex_dir / "README.md").write_text("# Good: description")
        (examples_dir / "README.md").write_text(
            "| [`good`](wrong-target/) | description |\n"
        )

        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.validation.check_documentation",
                "--root",
                str(self.root_dir),
            ],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("wrong-target", result.stdout)

    def test_example_index_extra_columns_keep_link_title_and_duplicate_guards(self):
        examples = self.root_dir / "benchmarks"
        case = examples / "good"
        case.mkdir(parents=True)
        (case / "README.md").write_text("# Good: description\n")
        index = examples / "README.md"
        row = "| [`good`](good/) | description | lifecycle | Python, R |\n"
        index.write_text(row)
        self.assertEqual(OWNER_EDITORIAL.validate_examples_index(self.root_dir), [])
        index.write_text(row + row)
        self.assertIn(
            "duplicate index entry for good",
            "\n".join(OWNER_EDITORIAL.validate_examples_index(self.root_dir)),
        )
        index.write_text(row.replace("(good/)", "(wrong-target/)"))
        self.assertIn(
            "wrong-target",
            "\n".join(OWNER_EDITORIAL.validate_examples_index(self.root_dir)),
        )
        index.write_text(row.replace("description", "wrong description"))
        self.assertTrue(OWNER_EDITORIAL.validate_examples_index(self.root_dir))

    def test_example_index_is_derived_not_committed(self):
        # The committed README carries no index table; the index is derived
        # from the case directories, so a case absent from the committed
        # README is still indexed.
        ex_dir = self.root_dir / "benchmarks"
        ex_dir.mkdir(parents=True, exist_ok=True)
        (ex_dir / "README.md").write_text("# Index\n\n## Benchmarks\n")
        good_ex = ex_dir / "good"
        good_ex.mkdir()
        readme = good_ex / "README.md"
        readme.write_text(
            "# Good description\n"
            "[![Lifecycle: draft](https://example.com/badge)](https://example.com/lifecycle)\n"
        )
        (good_ex / "spec.yaml").write_text("{}")
        self.assertEqual(OWNER_EDITORIAL.validate_examples_index(self.root_dir), [])
        # A case the derived index cannot render is reported, not dropped.
        readme.write_text("# Good description\n")
        self.assertEqual(
            OWNER_EDITORIAL.validate_examples_index(self.root_dir),
            ["ERROR: cannot derive benchmark index: missing title/lifecycle: good"],
        )

    def test_csv_header_uses_output_columns(self):
        ex_dir = self.root_dir / "benchmarks" / "csv-output"
        ex_dir.mkdir(parents=True, exist_ok=True)
        (ex_dir / "spec.yaml").write_text(
            "output:\n  path: out.csv\n  columns: [c, a]\n"
            "columns:\n  - name: a\n  - name: b\n  - name: c\n"
        )
        (ex_dir / "expected").mkdir()
        (ex_dir / "expected" / "out.csv").write_text("c,a\n3,1\n")

        errors, _ = OWNER_REPOSITORY_ARTIFACTS.validate_examples_csv(self.root_dir)

        self.assertEqual(errors, [])

    def test_csv_header_mismatch(self):
        ex_dir = self.root_dir / "benchmarks" / "csv-bad"
        ex_dir.mkdir(parents=True, exist_ok=True)
        (ex_dir / "README.md").write_text("# CSV: bad header")
        (ex_dir / "spec.yaml").write_text(
            "output:\n  path: out.csv\n  columns: [a, c]\n"
            "columns:\n  - name: a\n  - name: b\n  - name: c\n"
        )
        (ex_dir / "input").mkdir()
        (ex_dir / "expected").mkdir()
        (ex_dir / "expected" / "out.csv").write_text("a,b,c\n1,2,3")
        (self.root_dir / "benchmarks" / "README.md").write_text(
            "| [`csv-bad`](csv-bad/) | bad header |\n"
        )

        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.validation.validate_repository",
                "--root",
                str(self.root_dir),
            ],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("csv-bad", result.stdout)
        self.assertIn("header", result.stdout.lower())

    def write_producer_example(self, name):
        ex_dir = self.root_dir / "benchmarks" / name
        (ex_dir / "expected").mkdir(parents=True)
        (ex_dir / "spec_dm.yaml").write_text(
            "output:\n  path: dm.csv\n  columns: [a]\ncolumns:\n  - name: a\n"
        )
        (ex_dir / "spec_supp.yaml").write_text(
            "input:\n  DM: {path: dm.csv, schema: spec_dm.yaml}\n"
            "output:\n  path: supp.csv\n  columns: [b]\n"
            "columns:\n  - name: b\n"
        )
        (ex_dir / "expected" / "dm.csv").write_text("a\n1\n")
        (ex_dir / "expected" / "supp.csv").write_text("b\n2\n")
        return ex_dir

    def test_csv_golden_of_a_sibling_producer_is_accepted(self):
        self.write_producer_example("producer-output")

        errors, _ = OWNER_REPOSITORY_ARTIFACTS.validate_examples_csv(self.root_dir)

        self.assertEqual(errors, [])

    def test_csv_golden_no_spec_declares_is_still_rejected(self):
        ex_dir = self.write_producer_example("producer-stray")
        (ex_dir / "expected" / "other.csv").write_text("c\n3\n")

        errors, _ = OWNER_REPOSITORY_ARTIFACTS.validate_examples_csv(self.root_dir)

        message = "\n".join(errors)
        self.assertIn("producer-stray/other.csv: expected artifact name", message)
        self.assertNotIn("dm.csv: expected artifact name", message)
        self.assertNotIn("supp.csv: expected artifact name", message)

    def test_spec_structural_unknown_field(self):
        ex_dir = self.root_dir / "benchmarks" / "spec-bad-field"
        ex_dir.mkdir(parents=True, exist_ok=True)
        (self.root_dir / "yaml" / "schema.yaml").write_text("""version: "1.0"
root_class:
  - schema_version: {type: str, required: true}
  - domain: {type: str, required: true}
  - input: {type: str, required: true}
  - keys: {type: str, required: true}
  - columns: {type: str, required: true}
""")
        (ex_dir / "README.md").write_text("# Bad: field")
        # Missing domain and input which are required, and has an unknown field 'bad_field'
        (ex_dir / "spec.yaml").write_text("""schema_version: "1.0"
domain: "test"
input: {}
keys: []
columns: []
bad_field: "what"
""")
        (ex_dir / "input").mkdir()
        (ex_dir / "expected").mkdir()
        (ex_dir / "expected" / "out.csv").write_text("a\n1")
        (self.root_dir / "benchmarks" / "README.md").write_text(
            "| [`spec-bad-field`](spec-bad-field/) | field |\n"
        )

        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.validation.validate_repository",
                "--root",
                str(self.root_dir),
            ],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("bad_field", result.stdout)

    @patch.object(
        OWNER_REPOSITORY_STRUCTURE,
        "validate_spec_document",
        side_effect=typed_schema_control,
    )
    def test_multiple_spec_variants_are_discovered_and_validated(self, _native):
        ex_dir = self.root_dir / "benchmarks" / "variant-example"
        (ex_dir / "input").mkdir(parents=True)
        (ex_dir / "expected").mkdir()
        (ex_dir / "README.md").write_text(
            "# Variant example\n"
            "\n"
            "[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/variant-example.html)\n"
        )
        (ex_dir / "expected" / "out.csv").write_text("value\n1\n")
        (ex_dir / "spec_r.yaml").write_text("value: valid\n")
        (ex_dir / "spec_py.yaml").write_text("value: valid\nbad_field: true\n")
        env = {
            "version": "1.0",
            "classes": {"root_class": [{"value": {"type": "str", "required": True}}]},
            "aliases": {},
            "registries": {},
        }

        paths = OWNER_EDITORIAL.example_spec_paths(ex_dir)
        self.assertEqual(
            [path.name for path in paths],
            ["spec_py.yaml", "spec_r.yaml"],
        )
        self.assertEqual(
            OWNER_REPOSITORY_STRUCTURE.validate_examples_layout(self.root_dir), []
        )
        errors = OWNER_REPOSITORY_STRUCTURE.validate_examples_structure(
            self.root_dir, env
        )
        self.assertTrue(errors)
        self.assertIn("spec_py.yaml", "\n".join(errors))
        self.assertIn("bad_field", "\n".join(errors))

    def test_layout_rejects_base_spec_mixed_with_variants(self):
        ex_dir = self.root_dir / "benchmarks" / "mixed-specs"
        (ex_dir / "input").mkdir(parents=True)
        (ex_dir / "expected").mkdir()
        (ex_dir / "README.md").write_text(
            "# Mixed specs\n"
            "\n"
            "[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/mixed-specs.html)\n"
        )
        (ex_dir / "expected" / "out.csv").write_text("value\n1\n")
        (ex_dir / "spec.yaml").write_text("value: base\n")
        (ex_dir / "spec_r.yaml").write_text("value: variant\n")

        errors = OWNER_REPOSITORY_STRUCTURE.validate_examples_layout(self.root_dir)
        self.assertTrue(errors)
        self.assertIn("cannot mix", "\n".join(errors))

    def test_parented_spec_files_are_levels_not_entries(self):
        ex_dir = self.root_dir / "benchmarks" / "leveled-specs"
        (ex_dir / "input").mkdir(parents=True)
        (ex_dir / "expected").mkdir()
        (ex_dir / "README.md").write_text(
            "# Leveled specs\n"
            "\n"
            "[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/leveled-specs.html)\n"
        )
        (ex_dir / "expected" / "out.csv").write_text("value\n1\n")
        (ex_dir / "spec_organization.yaml").write_text("value: valid\n")
        (ex_dir / "spec_study.yaml").write_text(
            "value: valid\nparents: spec_organization.yaml\n"
        )

        entries = OWNER_REPOSITORY_ARTIFACTS.example_entry_specs(ex_dir)
        self.assertEqual([path.name for path in entries], ["spec_study.yaml"])
        self.assertEqual(
            OWNER_REPOSITORY_ARTIFACTS.expected_resolved_path(ex_dir, entries[0]).name,
            "spec_resolved.yaml",
        )
        self.assertEqual(
            OWNER_REPOSITORY_STRUCTURE.validate_examples_layout(self.root_dir), []
        )

    def test_schema_linked_spec_files_are_producers_not_entries(self):
        ex_dir = self.root_dir / "benchmarks" / "producer-specs"
        (ex_dir / "input").mkdir(parents=True)
        (ex_dir / "expected").mkdir()
        (ex_dir / "expected" / "out.csv").write_text("value\n1\n")
        (ex_dir / "spec_dm.yaml").write_text("value: valid\n")
        (ex_dir / "spec_suppdm.yaml").write_text(
            "value: valid\ninput:\n  DM: {path: dm.csv, schema: spec_dm.yaml}\n"
        )

        entries = OWNER_REPOSITORY_ARTIFACTS.example_entry_specs(ex_dir)
        self.assertEqual([path.name for path in entries], ["spec_suppdm.yaml"])

    def test_readme_is_a_data_contract_without_badge_configuration(self):
        ex_dir = self.root_dir / "benchmarks" / "metadata-owned"
        (ex_dir / "expected").mkdir(parents=True)
        (ex_dir / "README.md").write_text(
            "# Collected values\n\nThe value is the collected response.\n"
        )
        self.assertEqual(OWNER_EDITORIAL.validate_example_readmes(self.root_dir), [])
        self.assertEqual(
            OWNER_EDITORIAL.validate_examples_readme_presence(self.root_dir), []
        )

    def test_layout_skips_missing_readme_presence_check(self):
        # Issue #184 round 1: a missing README fails only in the docs-lint
        # gate (validate_examples_readme_presence), not in the layout gate.
        ex_dir = self.root_dir / "benchmarks" / "no-readme"
        (ex_dir / "input").mkdir(parents=True)
        (ex_dir / "expected").mkdir()
        (ex_dir / "spec.yaml").write_text("value: valid\n")
        (ex_dir / "expected" / "out.csv").write_text("value\n1\n")
        self.assertEqual(
            OWNER_REPOSITORY_STRUCTURE.validate_examples_layout(self.root_dir), []
        )
        presence = OWNER_EDITORIAL.validate_examples_readme_presence(self.root_dir)
        self.assertIn("missing README.md", "\n".join(presence))

    def test_layout_skips_missing_how_to_fix_check(self):
        # Issue #184 round 1: a negative README without '## How to fix'
        # fails only in the docs-lint gate, not in the layout gate.
        ex_dir = self.root_dir / "benchmarks" / "negative-no-fix"
        (ex_dir / "input").mkdir(parents=True)
        (ex_dir / "expected").mkdir()
        (ex_dir / "spec.yaml").write_text("value: valid\n")
        (ex_dir / "expected" / "error.yaml").write_text("{}")
        badge = (
            "[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)]"
            "(https://elong0527.github.io/yamaa/benchmark/negative-no-fix.html)"
        )
        (ex_dir / "README.md").write_text(f"# No fix\n\n{badge}\n")
        self.assertEqual(
            OWNER_REPOSITORY_STRUCTURE.validate_examples_layout(self.root_dir), []
        )
        presence = OWNER_EDITORIAL.validate_examples_readme_presence(self.root_dir)
        self.assertIn("How to fix", "\n".join(presence))

    def test_readme_width_has_no_configuration_badge_exception(self):
        ex_dir = self.root_dir / "benchmarks" / "badge-width"
        (ex_dir / "expected").mkdir(parents=True)
        badge = (
            "[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)]"
            "(https://elong0527.github.io/yamaa/benchmark/badge-width.html)"
        )
        lifecycle = (
            "[![Lifecycle: finalized]"
            "(https://img.shields.io/badge/Lifecycle-finalized-brightgreen)]"
            "(https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)"
        )
        combined = f"{badge} {lifecycle}"
        self.assertGreater(len(combined), 79)
        (ex_dir / "README.md").write_text(f"# Badge width\n\n{combined}\n")
        self.assertTrue(OWNER_EDITORIAL.validate_example_readmes(self.root_dir))
        (ex_dir / "README.md").write_text(f"# Long prose\n\n{'x' * 80}\n")
        errors = OWNER_EDITORIAL.validate_example_readmes(self.root_dir)
        self.assertIn("maximum is 79", "\n".join(errors))

    def test_empty_spec_is_rejected(self):
        ex_dir = self.root_dir / "benchmarks" / "empty-spec"
        ex_dir.mkdir(parents=True)
        (ex_dir / "spec.yaml").write_text("")
        env = {
            "version": "1.0",
            "classes": {"root_class": []},
            "aliases": {},
            "registries": {},
        }

        errors = OWNER_REPOSITORY_STRUCTURE.validate_examples_structure(
            self.root_dir, env
        )
        self.assertTrue(errors)
        self.assertIn("mapping", "\n".join(errors))

    def test_spec_schema_version_must_match_bundle(self):
        ex_dir = self.root_dir / "benchmarks" / "wrong-version"
        ex_dir.mkdir(parents=True)
        (ex_dir / "spec.yaml").write_text('schema_version: "2.0"\n')
        env = {
            "version": "1.0",
            "classes": {
                "root_class": [
                    {
                        "schema_version": {
                            "type": "str",
                            "required": True,
                        }
                    }
                ]
            },
            "aliases": {},
            "registries": {},
        }

        errors = OWNER_REPOSITORY_STRUCTURE.validate_examples_structure(
            self.root_dir, env
        )
        self.assertTrue(errors)
        self.assertIn("2.0", "\n".join(errors))
        self.assertIn("1.0", "\n".join(errors))

    @patch.object(
        OWNER_REPOSITORY_STRUCTURE,
        "validate_spec_document",
        side_effect=typed_schema_control,
    )
    def test_negative_spec_rejects_unrelated_structural_errors(self, _native):
        ex_dir = self.root_dir / "benchmarks" / "negative-unrelated"
        (ex_dir / "expected").mkdir(parents=True)
        (ex_dir / "spec.yaml").write_text(
            "columns:\n  - name: COUNTRY\n    derivation: valid\nbad_root_field: true\n"
        )
        (ex_dir / "expected" / "error.yaml").write_text(
            "phase: validation\ncondition: invalid_field_type\n"
            "spec_paths: [columns.COUNTRY.derivation]\n"
        )
        env = {
            "classes": {
                "root_class": [
                    {"columns": {"type": "list[column_class]", "required": True}}
                ],
                "column_class": [
                    {"name": {"type": "str", "required": True}},
                    {"derivation": {"type": "str", "required": True}},
                ],
            },
            "aliases": {},
            "registries": {},
        }

        errors = OWNER_REPOSITORY_STRUCTURE.validate_examples_structure(
            self.root_dir, env
        )
        self.assertTrue(errors)
        self.assertIn("bad_root_field", "\n".join(errors))

    @patch.object(
        OWNER_REPOSITORY_STRUCTURE,
        "validate_spec_document",
        side_effect=typed_schema_control,
    )
    def test_negative_spec_allows_declared_structural_failure(self, _native):
        ex_dir = self.root_dir / "benchmarks" / "negative-declared"
        (ex_dir / "expected").mkdir(parents=True)
        (ex_dir / "spec.yaml").write_text(
            "columns:\n  - name: COUNTRY\n    label: Country\n    derivation: {nested: value}\n"
        )
        (ex_dir / "expected" / "error.yaml").write_text(
            "phase: validation\ncondition: invalid_field_type\n"
            "spec_paths: [columns.COUNTRY.derivation]\n"
        )
        env = {
            "classes": {
                "root_class": [
                    {"columns": {"type": "list[column_class]", "required": True}}
                ],
                "column_class": [
                    {"name": {"type": "str", "required": True}},
                    {"label": {"type": "str", "required": False}},
                    {"derivation": {"type": "str", "required": True}},
                ],
            },
            "aliases": {},
            "registries": {},
        }

        errors = OWNER_REPOSITORY_STRUCTURE.validate_examples_structure(
            self.root_dir, env
        )
        self.assertEqual(errors, [])

    @patch.object(
        OWNER_REPOSITORY_STRUCTURE,
        "validate_spec_document",
        side_effect=typed_schema_control,
    )
    def test_negative_path_matches_list_item(self, _native):
        ex_dir = self.root_dir / "benchmarks" / "negative-list-path"
        (ex_dir / "expected").mkdir(parents=True)
        (ex_dir / "spec.yaml").write_text("keys: [wrong]\n")
        (ex_dir / "expected" / "error.yaml").write_text(
            "phase: validation\ncondition: invalid_field_type\nspec_paths: [keys]\n"
        )
        env = {
            "version": "1.0",
            "classes": {
                "root_class": [{"keys": {"type": "list[int]", "required": True}}]
            },
            "aliases": {},
            "registries": {},
        }

        errors = OWNER_REPOSITORY_STRUCTURE.validate_examples_structure(
            self.root_dir, env
        )
        self.assertEqual(errors, [])

    def test_expected_error_contract_rejects_bad_phase_and_path(self):
        ex_dir = self.root_dir / "benchmarks" / "negative-contract"
        (ex_dir / "expected").mkdir(parents=True)
        (ex_dir / "spec.yaml").write_text("columns: [{name: A}]\n")
        (ex_dir / "expected" / "error.yaml").write_text(
            "phase: someday\ncondition: Bad Name\n"
            "spec_paths: [columns.MISSING]\ncontext: text\n"
        )

        errors = OWNER_REPOSITORY_MANIFESTS.validate_expected_error_contracts(
            self.root_dir
        )

        message = "\n".join(errors)
        self.assertIn("phase", message)
        self.assertIn("snake-case", message)
        self.assertIn("does not exist", message)
        self.assertIn("context", message)

    def test_csv_shape_rejects_duplicate_header_and_short_row(self):
        csv_dir = self.root_dir / "benchmarks" / "csv-shape" / "input"
        csv_dir.mkdir(parents=True)
        (csv_dir / "input.csv").write_text("A,A\n1\n")

        errors = OWNER_REPOSITORY_ARTIFACTS.validate_csv_shapes(self.root_dir)

        message = "\n".join(errors)
        self.assertIn("duplicate header", message)
        self.assertIn("expected 2 fields, got 1", message)

    def test_readme_contract_rejects_schema_vocabulary_and_extra_section(self):
        ex_dir = self.root_dir / "benchmarks" / "readme-contract"
        (ex_dir / "expected").mkdir(parents=True)
        (ex_dir / "README.md").write_text(
            "# Test: output\n\nThe schema derivation is shown.\n\n## Notes\n"
        )

        errors = OWNER_EDITORIAL.validate_example_readmes(self.root_dir)

        message = "\n".join(errors)
        self.assertIn("schema vocabulary", message)
        self.assertIn("unsupported level-two section", message)

    def test_integration_full_corpus(self):

        real_root = ROOT
        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.validation.validate_repository",
                "--root",
                str(real_root),
            ],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("PASS", result.stdout)


class TestCsvProfile(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        OWNER_REPOSITORY_ARTIFACTS._ensure_csv_binding()

    def parse_render(self, data):
        records = OWNER_REPOSITORY_ARTIFACTS.scan_records(data)
        return OWNER_REPOSITORY_ARTIFACTS.render_records(
            records[0], records[1:]
        ).decode("utf-8")

    def test_bare_and_quoted_empty_are_both_missing(self):
        records = OWNER_REPOSITORY_ARTIFACTS.scan_records('A,B\n1,\n2,""\n')
        self.assertIsNone(records[1][1])
        self.assertIsNone(records[2][1])

    def test_quoted_field_carries_delimiter_quote_and_newline(self):
        data = 'A\n"x, y"\n"say ""hi"""\n"two\nlines"\n'
        records = OWNER_REPOSITORY_ARTIFACTS.scan_records(data)
        self.assertEqual(
            [record[0] for record in records[1:]],
            ["x, y", 'say "hi"', "two\nlines"],
        )
        self.assertEqual(self.parse_render(data), data)

    def test_carriage_return_terminator_is_rejected(self):
        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw) / "adsl.csv"
            path.write_bytes(b"A,B\r\n1,2\r\n")
            errors = OWNER_REPOSITORY_ARTIFACTS.validate_csv_artifact(
                path, "ex/adsl.csv", {}
            )
        self.assertTrue(any("U+000A" in error for error in errors), errors)

    def test_unterminated_final_record_is_rejected(self):
        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw) / "adsl.csv"
            path.write_bytes(b"A,B\n1,2")
            errors = OWNER_REPOSITORY_ARTIFACTS.validate_csv_artifact(
                path, "ex/adsl.csv", {}
            )
        self.assertTrue(any("U+000A" in error for error in errors), errors)

    def test_unterminated_quote_is_rejected(self):
        with self.assertRaises(ValueError):
            OWNER_REPOSITORY_ARTIFACTS.scan_records('A,B\n1,"open\n')

    def test_needless_quoting_does_not_render_back(self):
        data = 'A,B\n"1",2\n'
        self.assertNotEqual(self.parse_render(data), data)

    def test_zero_row_artifact_is_the_header_alone(self):
        data = "STUDYID,USUBJID\n"
        self.assertEqual(len(OWNER_REPOSITORY_ARTIFACTS.scan_records(data)), 1)
        self.assertEqual(self.parse_render(data), data)

    def test_float_text_omits_a_trailing_zero_decimal(self):
        self.assertIsNone(OWNER_REPOSITORY_ARTIFACTS.canonical_float_text("10"))
        self.assertIn("10", OWNER_REPOSITORY_ARTIFACTS.canonical_float_text("10.0"))
        self.assertIsNone(
            OWNER_REPOSITORY_ARTIFACTS.canonical_float_text("73.66666666666667")
        )

    def test_float_text_is_positional_and_never_exponential(self):
        # A shorter string that round-trips is not the text: the digits are
        # shortest, the notation is fixed, so one value has one spelling.
        self.assertIsNone(OWNER_REPOSITORY_ARTIFACTS.canonical_float_text("0.0001"))
        self.assertIn("0.0001", OWNER_REPOSITORY_ARTIFACTS.canonical_float_text("1e-4"))
        self.assertIsNone(
            OWNER_REPOSITORY_ARTIFACTS.canonical_float_text("100000000000000000000")
        )
        self.assertIn(
            "100000000000000000000",
            OWNER_REPOSITORY_ARTIFACTS.canonical_float_text("1e+20"),
        )
        self.assertIsNone(OWNER_REPOSITORY_ARTIFACTS.canonical_float_text("0.0000001"))

    def test_extreme_precision_and_magnitude_do_not_raise(self):
        # REQ-0744 admits at most 18 fractional digits. The exact stored
        # binary64 0.1 rounds to this independently authored fixed text.
        self.assertIsNone(
            OWNER_REPOSITORY_ARTIFACTS.canonical_float_text("0.100000000000000006", 18)
        )
        self.assertIsNotNone(
            OWNER_REPOSITORY_ARTIFACTS.canonical_float_text("0.10000000000000000", 18)
        )
        # The magnitude still exceeds the default decimal context. An
        # uncaught InvalidOperation would abort the whole run.
        self.assertIsNotNone(
            OWNER_REPOSITORY_ARTIFACTS.canonical_float_text("1e300", 18)
        )
        self.assertIsNone(OWNER_REPOSITORY_ARTIFACTS.canonical_float_text("0.0313", 4))

    def test_temporal_text_follows_the_r016_grammar(self):
        self.assertIsNone(
            OWNER_REPOSITORY_ARTIFACTS.canonical_temporal_text("2025-01-15", "date")
        )
        self.assertIsNone(
            OWNER_REPOSITORY_ARTIFACTS.canonical_temporal_text(
                "2024-01-01T08:00:00", "datetime"
            )
        )
        for value, kind in [
            ("2025-1-2", "date"),
            ("2025-99-99", "date"),
            ("2025-02-30", "date"),
            ("totally invalid", "date"),
            ("2025-01-12T14:00:00Z", "datetime"),
            ("2025-01-12T14:00:00+02:00", "datetime"),
            ("2025-01-12T14:00:00.5", "datetime"),
            ("2025-01-12T23:59:60", "datetime"),
            ("2025-01-12", "datetime"),
        ]:
            self.assertIsNotNone(
                OWNER_REPOSITORY_ARTIFACTS.canonical_temporal_text(value, kind),
                f"{value!r} should not be canonical {kind} text",
            )

    def test_declared_precision_fixes_the_written_width(self):
        self.assertIsNone(OWNER_REPOSITORY_ARTIFACTS.canonical_float_text("25.0000", 4))
        self.assertIsNone(OWNER_REPOSITORY_ARTIFACTS.canonical_float_text("0", 0))
        self.assertIsNotNone(OWNER_REPOSITORY_ARTIFACTS.canonical_float_text("25", 4))
        self.assertIsNotNone(
            OWNER_REPOSITORY_ARTIFACTS.canonical_float_text("25.000", 4)
        )

    def test_exact_ties_round_away_from_zero(self):
        quantize = OWNER_DECIMAL.Decimal(1).scaleb(-2)
        rounding = OWNER_DECIMAL.ROUND_HALF_UP
        self.assertEqual(
            str(OWNER_DECIMAL.Decimal(0.125).quantize(quantize, rounding)),
            "0.13",
        )
        self.assertEqual(
            str(OWNER_DECIMAL.Decimal(-0.125).quantize(quantize, rounding)),
            "-0.13",
        )

    def test_an_unrepresentable_tie_rounds_on_the_exact_value(self):
        # 2.675 is stored below the decimal it is written as, so there is no
        # tie to break and the value rounds down. Rounding the shortened text
        # instead would report 2.68.
        self.assertEqual(
            str(
                OWNER_DECIMAL.Decimal(2.675).quantize(
                    OWNER_DECIMAL.Decimal(1).scaleb(-2),
                    OWNER_DECIMAL.ROUND_HALF_UP,
                )
            ),
            "2.67",
        )

    def test_non_canonical_int_text_is_reported(self):
        self.assertTrue(OWNER_REPOSITORY_ARTIFACTS.CANONICAL_INT.fullmatch("0"))
        self.assertTrue(OWNER_REPOSITORY_ARTIFACTS.CANONICAL_INT.fullmatch("-12"))
        self.assertFalse(OWNER_REPOSITORY_ARTIFACTS.CANONICAL_INT.fullmatch("007"))
        self.assertFalse(OWNER_REPOSITORY_ARTIFACTS.CANONICAL_INT.fullmatch("+1"))
        self.assertFalse(OWNER_REPOSITORY_ARTIFACTS.CANONICAL_INT.fullmatch("-0"))

    def test_artifact_check_reports_bytes_and_values(self):
        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw) / "adsl.csv"
            path.write_bytes(b"STUDYID,AVAL\r\nS1,10.0\r\n")
            spec = {
                "output": {"profile": "csv", "columns": ["STUDYID", "AVAL"]},
                "columns": [
                    {"name": "STUDYID", "type": "str"},
                    {"name": "AVAL", "type": "float"},
                ],
            }
            errors = OWNER_REPOSITORY_ARTIFACTS.validate_csv_artifact(
                path, "ex/adsl.csv", spec
            )
        self.assertTrue(any("U+000A" in error for error in errors), errors)

    def test_artifact_check_accepts_a_conforming_artifact(self):
        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw) / "adsl.csv"
            path.write_bytes(b'STUDYID,COMMENT,AVAL\nS1,"has, comma",10\nS1,,\n')
            spec = {
                "output": {"profile": "csv", "columns": ["STUDYID", "COMMENT", "AVAL"]},
                "columns": [
                    {"name": "STUDYID", "type": "str"},
                    {"name": "COMMENT", "type": "str"},
                    {"name": "AVAL", "type": "float"},
                ],
            }
            errors = OWNER_REPOSITORY_ARTIFACTS.validate_csv_artifact(
                path, "ex/adsl.csv", spec
            )
        self.assertEqual(errors, [])

    def test_decimals_needs_a_csv_artifact_path(self):
        base = {
            "schema_version": "1.0",
            "domain": "ADSL",
            "input": {"SRC": "input/adsl.csv"},
            "base": "SRC",
            "keys": ["USUBJID"],
            "columns": [
                {
                    "name": "USUBJID",
                    "type": "str",
                    "label": "Subject",
                    "derivation": {"source": "SRC.USUBJID"},
                },
            ],
        }

        def check(output):
            spec = copy.deepcopy(base)
            spec["output"] = output
            return "\n".join(CONTROLS.names(spec, "ex/spec.yaml"))

        self.assertIn(
            "decimals_not_applicable",
            check({"path": "adsl.parquet", "decimals": 4, "columns": ["USUBJID"]}),
        )
        self.assertNotIn(
            "decimals_not_applicable",
            check({"path": "adsl.csv", "decimals": 4, "columns": ["USUBJID"]}),
        )
        self.assertIn(
            "an integer from 0 through 18",
            check({"path": "adsl.csv", "decimals": -1, "columns": ["USUBJID"]}),
        )

    def test_unknown_extension_names_no_profile(self):
        # The mapping is closed: an unrecognized extension is a failure, not
        # a fall back to the other profile.
        self.assertEqual(
            OWNER_REPOSITORY_ARTIFACTS.artifact_profile({"path": "adsl.csv"}), "csv"
        )
        self.assertEqual(
            OWNER_REPOSITORY_ARTIFACTS.artifact_profile({"path": "out/adsl.parquet"}),
            "parquet",
        )
        self.assertEqual(
            OWNER_REPOSITORY_ARTIFACTS.artifact_profile({"path": "adsl.CSV"}), "csv"
        )
        for path in ["adsl.txt", "adsl", "adsl.csv.gz", ""]:
            self.assertIsNone(
                OWNER_REPOSITORY_ARTIFACTS.artifact_profile({"path": path}),
                f"{path!r} should name no profile",
            )
        self.assertIsNone(OWNER_REPOSITORY_ARTIFACTS.artifact_profile({}))

    def test_byte_order_mark_is_rejected(self):
        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw) / "adsl.csv"
            path.write_bytes(b"\xef\xbb\xbfSTUDYID\nS1\n")
            errors = OWNER_REPOSITORY_ARTIFACTS.validate_csv_artifact(
                path, "ex/adsl.csv", {"output": {}, "columns": []}
            )
        self.assertTrue(any("byte-order mark" in error for error in errors), errors)


class TestNativeTemporalFixtureAdmission(unittest.TestCase):
    def test_tagged_function_literals_keep_calendar_and_collected_precision_truth(self):
        for kind, text, expected in (
            ("date", "2025-01-15", True),
            ("date", "2025-02-30", False),
            ("datetime", "2025-01-15T03:04", True),
            ("datetime", "2025-01-15T03:04:05", True),
            ("datetime", "2025-01-15T03:04:60", False),
            ("datetime", "2025-02-30T03:04", False),
        ):
            with self.subTest(kind=kind, text=text):
                self.assertIs(
                    OWNER_REPOSITORY_SCHEMA.valid_temporal_literal(kind, text), expected
                )


class TestSourceProfile(unittest.TestCase):
    """R023: what a delimited source may spell, and what it may not."""

    def condition(self, data):
        try:
            OWNER_REPOSITORY_ARTIFACTS.parse_source_profile(data)
        except OWNER_REPOSITORY_ARTIFACTS.SourceProfileError as exc:
            return exc.condition, exc.record, exc.field
        return None

    def test_both_terminators_deliver_the_same_records(self):
        self.assertEqual(
            OWNER_REPOSITORY_ARTIFACTS.parse_source_profile("A,B\n1,2\n"),
            OWNER_REPOSITORY_ARTIFACTS.parse_source_profile("A,B\r\n1,2\r\n"),
        )

    def test_final_record_may_omit_its_terminator(self):
        self.assertEqual(
            OWNER_REPOSITORY_ARTIFACTS.parse_source_profile("A,B\n1,2"),
            OWNER_REPOSITORY_ARTIFACTS.parse_source_profile("A,B\n1,2\n"),
        )

    def test_missing_and_empty_reach_r014_as_missing(self):
        records = OWNER_REPOSITORY_ARTIFACTS.parse_source_profile('A,B\n"",\n')
        self.assertEqual(records[1][0], None)
        self.assertEqual(records[1][1], None)

    def test_quoted_field_carries_delimiter_quote_and_newline(self):
        records = OWNER_REPOSITORY_ARTIFACTS.parse_source_profile(
            'A\n"x, y"\n"say ""hi"""\n"two\nlines"\n'
        )
        self.assertEqual(
            [record[0] for record in records[1:]],
            ["x, y", 'say "hi"', "two\nlines"],
        )

    def test_nothing_is_trimmed(self):
        records = OWNER_REPOSITORY_ARTIFACTS.parse_source_profile("A,B\n x , y \n")
        self.assertEqual(records[1], [" x ", " y "])

    def test_carriage_return_outside_a_terminator_is_rejected(self):
        for data in ['A\n"x\ry"\n', "A\nx\ry\n", "A\n1\r"]:
            self.assertEqual(self.condition(data)[0], "source_carriage_return", data)

    def test_malformed_quoting_is_rejected_with_coordinates(self):
        self.assertEqual(
            self.condition('A,B\n1,"x\n'),
            ("source_quote_unterminated", 2, 2),
        )
        self.assertEqual(
            self.condition('A,B\n1,x"y\n'),
            ("source_quote_in_bare_field", 2, 2),
        )
        self.assertEqual(
            self.condition('A,B\n1,"x"y\n'),
            ("source_text_after_quote", 2, 2),
        )

    def test_an_empty_file_has_no_header(self):
        self.assertEqual(OWNER_REPOSITORY_ARTIFACTS.parse_source_profile(""), [])

    def test_header_alone_is_a_source_with_no_records(self):
        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw) / "dm.csv"
            path.write_bytes(b"STUDYID,USUBJID\n")
            self.assertEqual(OWNER_REPOSITORY_ARTIFACTS.check_source_file(path), [])

    def test_header_and_width_conditions_are_reported(self):
        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw) / "dm.csv"
            path.write_bytes(b"A,A,\n1,2,3\n1,2\n")
            conditions = [
                condition
                for condition, _n, _d in OWNER_REPOSITORY_ARTIFACTS.check_source_file(
                    path
                )
            ]
        self.assertEqual(
            conditions,
            [
                "source_field_name_empty",
                "source_field_name_duplicate",
                "source_record_width",
            ],
        )

    def test_byte_order_mark_and_ill_formed_bytes_are_reported(self):
        with tempfile.TemporaryDirectory() as raw:
            mark = Path(raw) / "mark.csv"
            mark.write_bytes(b"\xef\xbb\xbfA\n1\n")
            ill = Path(raw) / "ill.csv"
            ill.write_bytes(b"A\n\xff\n")
            self.assertEqual(
                OWNER_REPOSITORY_ARTIFACTS.check_source_file(mark)[0][0],
                "source_byte_order_mark",
            )
            self.assertEqual(
                OWNER_REPOSITORY_ARTIFACTS.check_source_file(ill)[0][0], "invalid_text"
            )


class TestDeclaredSourceCondition(unittest.TestCase):
    """A negative example owns the malformed fixture it declares."""

    def setUp(self):
        self._temp = tempfile.TemporaryDirectory()
        self.root = Path(self._temp.name)
        self.addCleanup(self._temp.cleanup)

    def example(self, name, csv_bytes, condition=None):
        example_dir = self.root / "benchmarks" / name
        (example_dir / "input").mkdir(parents=True)
        (example_dir / "expected").mkdir(parents=True)
        (example_dir / "input" / "dm.csv").write_bytes(csv_bytes)
        if condition is not None:
            (example_dir / "expected" / "error.yaml").write_text(
                f"phase: ingest\ncondition: {condition}\n"
            )
        return example_dir

    def test_declared_condition_excuses_its_own_fixture(self):
        self.example(
            "negative-source-quote", b'A,B\n1,"x\n', "source_quote_unterminated"
        )
        self.assertEqual(OWNER_REPOSITORY_ARTIFACTS.validate_csv_shapes(self.root), [])

    def test_an_undeclared_malformed_fixture_still_fails(self):
        self.example("positive-source", b'A,B\n1,"x\n')
        errors = OWNER_REPOSITORY_ARTIFACTS.validate_csv_shapes(self.root)
        self.assertTrue(
            any("source_quote_unterminated" in error for error in errors),
            errors,
        )

    def test_a_declared_condition_no_fixture_reports_fails(self):
        self.example(
            "negative-source-quote", b"A,B\n1,2\n", "source_quote_unterminated"
        )
        errors = OWNER_REPOSITORY_ARTIFACTS.validate_csv_shapes(self.root)
        self.assertTrue(
            any("declared but no source fixture" in error for error in errors),
            errors,
        )

    def test_a_different_condition_than_declared_fails(self):
        self.example(
            "negative-source-quote", b"A,A\n1,2\n", "source_quote_unterminated"
        )
        errors = OWNER_REPOSITORY_ARTIFACTS.validate_csv_shapes(self.root)
        self.assertTrue(
            any("source_field_name_duplicate" in error for error in errors),
            errors,
        )


class TestSuiteSourceCoverage(unittest.TestCase):
    """The suite fixtures that carry R023's admitted spellings.

    Both are load-bearing: an editor who normalizes either file removes the
    coverage, so these tests say why the bytes are what they are.
    """

    root = ROOT

    def test_a_crlf_source_is_read_as_its_lf_twin(self):
        path = (
            self.root / "benchmarks" / "adam-adrs-best-response" / "input" / "adsl.csv"
        )
        raw = path.read_bytes()
        self.assertIn(b"\r\n", raw, "the suite needs one CRLF source")
        self.assertEqual(OWNER_REPOSITORY_ARTIFACTS.check_source_file(path), [])
        crlf = raw.decode("utf-8")
        self.assertEqual(
            OWNER_REPOSITORY_ARTIFACTS.parse_source_profile(crlf),
            OWNER_REPOSITORY_ARTIFACTS.parse_source_profile(crlf.replace("\r\n", "\n")),
        )

    def test_a_suite_source_reads_both_blanks_as_missing(self):
        path = (
            self.root
            / "benchmarks"
            / "adam-adsl-investigator-comment"
            / "input"
            / "dm.csv"
        )
        records = OWNER_REPOSITORY_ARTIFACTS.parse_source_profile(
            path.read_text(encoding="utf-8")
        )
        comments = [record[2] for record in records[1:]]
        self.assertNotIn("", comments, "no collected empty comment survives")
        self.assertEqual(comments.count(None), 2, "both blanks are missing")
        self.assertIn(
            ("Dose reduced, per protocol"),
            comments,
            "a comment carrying the delimiter",
        )


class TestSourceProfileDiagnostics(unittest.TestCase):
    """Where a rejected source says the defect is, and which files are read."""

    def test_ill_formed_bytes_report_their_own_record_and_field(self):
        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw) / "dm.csv"
            path.write_bytes(
                b"STUDYID,USUBJID,SITENM\n"
                b"CTX,CTX-01,Royal Infirmary\n"
                b"CTX,CTX-02,H\xf4pital Saint-Antoine\n"
            )
            findings = OWNER_REPOSITORY_ARTIFACTS.check_source_file(path)
        self.assertEqual(len(findings), 1)
        condition, record, detail = findings[0]
        self.assertEqual(condition, "invalid_text")
        self.assertEqual(record, 3)
        self.assertIn("field 3", detail)

    def test_coordinates_count_a_quoted_field_as_one_field(self):
        self.assertEqual(
            OWNER_REPOSITORY_ARTIFACTS.source_coordinates('A,B\n"x,y",'), (2, 2)
        )
        self.assertEqual(
            OWNER_REPOSITORY_ARTIFACTS.source_coordinates('A,B\n"two\nlines",'), (2, 2)
        )

    def test_an_uppercase_extension_is_scanned(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            (root / "benchmarks" / "upper" / "input").mkdir(parents=True)
            (root / "benchmarks" / "upper" / "input" / "DM.CSV").write_bytes(
                b"A,A\n1\n"
            )
            errors = OWNER_REPOSITORY_ARTIFACTS.validate_csv_shapes(root)
        message = "\n".join(errors)
        self.assertIn("source_field_name_duplicate", message)
        self.assertIn("source_record_width", message)

    def test_a_unicode_fixture_keeps_its_exemption_when_shouted(self):
        self.assertTrue(
            OWNER_EDITORIAL.is_unicode_fixture_data(Path("benchmarks/ex/input/DM.CSV"))
        )

    def test_the_validation_condition_is_not_asked_of_a_fixture(self):
        # A negative example declares source_profile_unknown against its
        # specification, so the fixture scan must not demand that some file
        # provoke it.
        self.assertNotIn(
            "source_profile_unknown", OWNER_REPOSITORY_ARTIFACTS.SOURCE_READ_CONDITIONS
        )
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            example = root / "benchmarks" / "negative-source-profile"
            (example / "input").mkdir(parents=True)
            (example / "expected").mkdir(parents=True)
            (example / "input" / "dm.txt").write_bytes(b"A\n1\n")
            (example / "expected" / "error.yaml").write_text(
                "phase: validation\ncondition: source_profile_unknown\n"
            )
            self.assertEqual(OWNER_REPOSITORY_ARTIFACTS.validate_csv_shapes(root), [])


ODM_HEADER = (
    "StudyOID,MetaDataVersionOID,SubjectKey,StudyEventOID,"
    "StudyEventRepeatKey,ItemGroupOID,ItemGroupRepeatKey,ItemOID,Value\n"
)


class TestRetiredOdmItemReferences(unittest.TestCase):
    """An ODM item is addressed by a source filter."""

    def env(self):
        env, errors = OWNER_REPOSITORY_SCHEMA.build_schema_env(ROOT)
        self.assertEqual(errors, [])
        return env

    def spec(self, dtc_derivation):
        return {
            "schema_version": "1.0",
            "domain": "DS",
            "keys": ["USUBJID"],
            "input": {"ODM": "input/odm.csv"},
            "output": {"path": "ds.csv", "columns": ["USUBJID", "DSSTDTC"]},
            "columns": [
                {
                    "name": "USUBJID",
                    "type": "str",
                    "label": "Unique Subject Identifier",
                    "derivation": {"source": "ODM.SubjectKey"},
                },
                {
                    "name": "DSSTDTC",
                    "type": "date",
                    "label": "Start Date/Time of Disposition Event",
                    "derivation": dtc_derivation,
                },
            ],
        }

    def findings(self, dtc_derivation, name="sdtm-ds-probe"):
        env = self.env()
        with tempfile.TemporaryDirectory() as raw:
            example = Path(raw) / "benchmarks" / name
            (example / "input").mkdir(parents=True)
            (example / "input" / "odm.csv").write_text(ODM_HEADER)
            spec_path = example / "spec.yaml"
            spec_path.write_text("{}\n")
            return SOURCE_CONTROLS.retired_odm(
                self.spec(dtc_derivation),
                f"{name}/spec.yaml",
                spec_path,
                env,
            )

    def test_accepts_a_source_filter_over_item_oid(self):
        self.assertEqual(
            self.findings(
                {
                    "source": {
                        "variable": "ODM.Value",
                        "filter": "ODM.ItemOID = 'IT.DS.DTC'",
                    },
                }
            ),
            [],
        )

    def test_rejects_an_item_hidden_in_the_variable_name(self):
        findings = self.findings({"source": "ODM.IT.DS.DTC"})

        self.assertEqual(len(findings), 1)
        self.assertIn("unknown_field", findings[0])
        self.assertEqual(findings[0].context["identifier"], "ODM.IT.DS.DTC")
        self.assertIn("columns.DSSTDTC.derivation.source", findings[0])

    def test_rejects_an_item_inside_a_case_branch(self):
        findings = self.findings(
            {
                "case": [
                    {
                        "when": "USUBJID = '001'",
                        "then": {"literal": "2024-01-01"},
                    },
                    {"otherwise": {"source": {"variable": "ODM.IT.DS.DTC"}}},
                ],
            }
        )

        self.assertEqual(len(findings), 1)
        self.assertIn("derivation.case[1].otherwise.source", findings[0])

    def test_the_migrated_specifications_carry_no_retired_reference(self):
        # These four carried the exemption the retirement shipped with. The
        # check now runs unexempted, so each has to pass it on its own; a
        # reintroduced contextual reference fails here and not only in a
        # whole-repository run.
        env = self.env()
        examples = ROOT / "benchmarks"
        migrated = [
            "adam-adsl-randomization/input/dm.schema.yaml",
            "schema-odm-form-items/spec.yaml",
            "sdtm-lb-findings/spec.yaml",
            "sdtm-lb-multiform/spec.yaml",
        ]

        for entry in migrated:
            spec_path = examples / entry
            self.assertTrue(spec_path.is_file(), entry)
            with open(spec_path) as handle:
                spec = yaml.safe_load(handle)
            self.assertEqual(
                SOURCE_CONTROLS.retired_odm(spec, entry, spec_path, env),
                [],
                entry,
            )

    def test_the_check_carries_no_exemption(self):
        # An exemption list nobody empties is a retirement nobody finishes.
        self.assertFalse(hasattr(VALIDATOR, "ODM_CONTEXTUAL_REFERENCE_MIGRATION"))


SCHEMA_HEADER = (
    "StudyOID,MetaDataVersionOID,SubjectKey,StudyEventOID,"
    "StudyEventRepeatKey,FormOID,FormRepeatKey,ItemGroupOID,"
    "ItemGroupRepeatKey,ItemOID,Value"
)


class TestOdmReads(unittest.TestCase):
    """An `odm` read is checked against its row scope and ODM input."""

    def spec(self, derivation, **extra):
        spec = {
            "schema_version": "1.0",
            "domain": "DM",
            "keys": ["USUBJID"],
            "input": {"ODM": "input/odm.csv"},
            "output": {"path": "dm.csv", "columns": ["USUBJID", "AGE"]},
            "columns": [
                {
                    "name": "USUBJID",
                    "type": "str",
                    "label": "Unique Subject Identifier",
                    "derivation": "ODM.SubjectKey",
                },
                {
                    "name": "AGE",
                    "type": "int",
                    "label": "Age",
                    "derivation": derivation,
                },
            ],
        }
        spec.update(extra)
        return spec

    def findings(self, spec, header=SCHEMA_HEADER):
        with tempfile.TemporaryDirectory() as raw:
            example = Path(raw) / "benchmarks" / "schema-odm-probe"
            (example / "input").mkdir(parents=True)
            (example / "input" / "odm.csv").write_text(header + "\n")
            spec_path = example / "spec.yaml"
            spec_path.write_text("{}\n")
            return SOURCE_CONTROLS.odm(spec, "schema-odm-probe/spec.yaml", spec_path)

    def test_accepts_a_read_over_a_header_in_any_case(self):
        header = SCHEMA_HEADER.upper() + ",SITEID"
        self.assertEqual(self.findings(self.spec({"odm": "ODM.IT.DM.AGE"}), header), [])

    def test_an_overridden_column_read_needs_no_odm_scope(self):
        spec = self.spec(
            {"odm": "ODM.IT.DM.AGE"},
            rows=[
                {
                    "id": "other_source",
                    "dataset": "DM",
                    "derivations": {"AGE": {"literal": 0}},
                }
            ],
        )
        spec["input"]["DM"] = "input/dm.csv"
        self.assertEqual(self.findings(spec), [])

        del spec["rows"][0]["derivations"]["AGE"]
        [finding] = self.findings(spec)
        self.assertEqual(finding.condition, "invalid_odm_context")

    def test_the_catalog_exposes_schema_fields_only(self):
        env, errors = OWNER_REPOSITORY_SCHEMA.build_schema_env(ROOT)
        self.assertEqual(errors, [])
        with tempfile.TemporaryDirectory() as raw:
            example = Path(raw) / "benchmarks" / "schema-odm-probe"
            (example / "input").mkdir(parents=True)
            (example / "input" / "odm.csv").write_text(
                SCHEMA_HEADER.upper() + ",SITEID\n"
            )
            catalog = SOURCE_CONTROLS.catalog(
                self.spec({"odm": "ODM.IT.DM.AGE"}),
                example / "spec.yaml",
                env,
            )
        self.assertEqual(
            catalog["ODM"],
            {field: "str" for field in OWNER_NATIVE_ODM.fields()},
        )

    def test_rejects_a_filter_naming_a_vendor_field(self):
        [finding] = self.findings(
            self.spec(
                {
                    "odm": {"item": "ODM.IT.DM.AGE", "filter": "ODM.SITEID = '9'"},
                }
            )
        )
        self.assertEqual(finding.condition, "unknown_field")
        self.assertEqual(
            finding.path,
            "schema-odm-probe/spec.yaml.columns.AGE.derivation.odm.filter",
        )

    def test_rejects_a_read_in_an_intermediate(self):
        spec = self.spec(
            {"literal": 1},
            intermediates=[
                {
                    "id": "AGES",
                    "dataset": "ODM",
                    "derivations": {"AGE": {"odm": "ODM.IT.DM.AGE"}},
                }
            ],
        )
        [finding] = self.findings(spec)
        self.assertEqual(finding.condition, "invalid_odm_context")
        self.assertEqual(
            finding.path,
            "schema-odm-probe/spec.yaml.intermediates[0].derivations.AGE.odm",
        )
        self.assertEqual(
            finding.context, {"dataset": "ODM", "location": "intermediate"}
        )

    def test_rejects_a_header_without_a_schema_field(self):
        header = SCHEMA_HEADER.replace("FormOID,FormRepeatKey,", "")
        [finding] = self.findings(self.spec({"odm": "ODM.IT.DM.AGE"}), header)
        self.assertEqual(finding.condition, "odm_schema_field_missing")
        self.assertEqual(
            finding.context,
            {"dataset": "ODM", "fields": ["FormOID", "FormRepeatKey"]},
        )

    def test_rejects_declared_types(self):
        spec = self.spec({"odm": "ODM.IT.DM.AGE"})
        spec["input"] = {
            "ODM": {"path": "input/odm.csv", "types": {"Value": "int"}},
        }
        [finding] = self.findings(spec)
        self.assertEqual(finding.condition, "odm_schema_field_type")
        self.assertEqual(finding.path, "schema-odm-probe/spec.yaml.input.ODM.types")


class TestRowPhaseDatasetReads(unittest.TestCase):
    """REQ-0156/REQ-0157 as the repository validator applies them."""

    def setUp(self):
        self.env, schema_errors = OWNER_REPOSITORY_SCHEMA.build_schema_env(ROOT)
        self.assertEqual(schema_errors, [])
        self.test_dir = tempfile.TemporaryDirectory()
        self.example_dir = Path(self.test_dir.name)
        self.input_dir = self.example_dir / "input"
        self.input_dir.mkdir()
        (self.input_dir / "main.csv").write_text(
            "ID,KIND,W\ns1,A,10\n", encoding="utf-8"
        )
        (self.input_dir / "aux.csv").write_text("ID,H\ns1,100\n", encoding="utf-8")
        self.spec_path = self.example_dir / "spec.yaml"

    def tearDown(self):
        self.test_dir.cleanup()

    def spec(self, derivations):
        return {
            "schema_version": "1.0",
            "domain": "OUT",
            "input": {
                "MAIN": {"path": "input/main.csv", "types": {"W": "float"}},
                "AUX": {"path": "input/aux.csv", "types": {"H": "float"}},
            },
            "keys": ["ID", "KIND"],
            "output": {"path": "out.csv", "columns": ["ID", "KIND", "VAL"]},
            "columns": [
                {
                    "name": "ID",
                    "type": "str",
                    "label": "Identifier",
                    "derivation": {"source": "MAIN.ID"},
                },
                {"name": "KIND", "type": "str", "label": "Kind"},
                {"name": "VAL", "type": "float", "label": "Value"},
            ],
            "rows": [
                {
                    "id": "r",
                    "dataset": "MAIN",
                    "derivations": derivations,
                }
            ],
        }

    def validate(self, derivations):
        return OWNER_REPOSITORY_SCHEMA.validate_spec_document(
            self.spec(derivations),
            "example/spec.yaml",
            self.spec_path,
            self.env,
        )

    def test_accepts_a_row_source_to_another_dataset(self):
        errors = self.validate(
            {
                "KIND": {"source": "MAIN.KIND"},
                "VAL": {"source": "AUX.H"},
            }
        )

        self.assertEqual(errors, [])

    def test_rejects_a_cross_dataset_identifier_in_a_row_formula(self):
        errors = self.validate(
            {
                "KIND": {"source": "MAIN.KIND"},
                "VAL": {"compute": {"expr": "MAIN.W / AUX.H"}},
            }
        )

        # REQ-0410: a row-phase formula names no qualified cross-dataset
        # identifier; the value must be bound to a column first.
        self.assertTrue(
            any(
                "unknown_field" in message
                and "rows[0]" in message
                and message.context == {"drivers": ["MAIN"], "identifier": "AUX.H"}
                for message in errors
            ),
            errors,
        )

    def test_rejects_an_unknown_field_in_a_row_source(self):
        errors = self.validate(
            {
                "KIND": {"source": "MAIN.KIND"},
                "VAL": {"source": "AUX.MISSING"},
            }
        )

        self.assertTrue(
            any(
                "unknown_field" in message and "rows[0]" in message
                for message in errors
            ),
            errors,
        )


class TestCorrelatedLookupFilters(unittest.TestCase):
    def specification(self, predicate, *, grouped=False):
        lookup = {
            "id": "PRIOR",
            "dataset": "OBS",
            "key": ["ID"],
            "filter": predicate,
        }
        spec = {
            "input": {"PLAN": "plan.csv", "OBS": "obs.csv", "OTHER": "other.csv"},
            "base": "PLAN",
            "keys": ["ID"],
            "columns": [
                {"name": "ID", "type": "str", "derivation": {"literal": "fixture"}},
                {"name": "AVAL", "type": "float", "derivation": "PRIOR.AVAL"},
            ],
            "intermediates": [lookup],
        }
        if grouped:
            spec["columns"][1].pop("derivation")
            spec["rows"] = [
                {
                    "id": "planned",
                    "dataset": "PLAN",
                    "group_by": ["PLAN.ID"],
                    "derivations": {"AVAL": "PRIOR.AVAL"},
                }
            ]
        return spec

    def test_only_current_driver_can_correlate(self):
        for qualifier, condition in [("PLAN", None), ("OTHER", "unknown_field")]:
            with self.subTest(qualifier=qualifier):
                errors = CONTROLS.document(
                    self.specification(f"OBS.VISIT < {qualifier}.VISIT"),
                    "spec",
                    None,
                    sources={
                        name: {"ID": "str", "VISIT": "int", "AVAL": "float"}
                        for name in ("PLAN", "OBS", "OTHER")
                    },
                )
                self.assertEqual(
                    [e.condition for e in errors],
                    [] if condition is None else [condition],
                )

    def test_grouped_correlation_requires_a_group_key(self):
        errors = CONTROLS.document(
            self.specification("OBS.VISIT < PLAN.VISIT", grouped=True),
            "spec",
            None,
            sources={
                name: {"ID": "str", "VISIT": "int", "AVAL": "float"}
                for name in ("PLAN", "OBS", "OTHER")
            },
        )
        self.assertEqual([e.condition for e in errors], ["unknown_field"])
        self.assertEqual(
            errors[0].context, {"identifier": "PLAN.VISIT", "intermediate": "PRIOR"}
        )


class TestFlagPredicates(unittest.TestCase):
    """Both flag spellings reach the predicate checks (REQ-1256)."""

    def errors(self, flag):
        condition = flag if isinstance(flag, str) else flag["condition"]
        return CONTROLS.predicate(
            condition,
            "spec.columns.ELDFL.derivation.flag.condition",
            CONTROLS.catalog({"AGE": "int"}),
        )

    def test_both_spellings_check_the_condition_at_its_field(self):
        # R006 expands a bare string to the mapping form, so both report
        # at the canonical condition path, as the engine does.
        for flag in ("AGE >> 65", {"condition": "AGE >> 65", "false_value": "N"}):
            with self.subTest(flag=flag):
                [error] = self.errors(flag)
                self.assertEqual(error.condition, "invalid_predicate")
                self.assertEqual(
                    error.path, "spec.columns.ELDFL.derivation.flag.condition"
                )

    def test_both_spellings_resolve_their_identifiers(self):
        for flag in ("AGEX >= 65", {"condition": "AGEX >= 65"}):
            with self.subTest(flag=flag):
                self.assertEqual(
                    [e.condition for e in self.errors(flag)], ["unknown_field"]
                )

    def test_both_spellings_name_their_dependencies(self):
        for flag in ("AOCCSEQ = 1", {"condition": "AOCCSEQ = 1"}):
            with self.subTest(flag=flag):
                self.assertEqual(
                    sorted(
                        CONTROLS.identifiers(
                            "predicate",
                            flag if isinstance(flag, str) else flag["condition"],
                        )
                    ),
                    ["AOCCSEQ"],
                )


class TestKeyColumnOrder(unittest.TestCase):
    """Populated DOMAIN, STUDYID, USUBJID columns keep that order everywhere."""

    def example(self, raw, name, declared, artifact, header):
        example = Path(raw) / "benchmarks" / name
        (example / "expected").mkdir(parents=True)
        (example / "spec.yaml").write_text(
            'schema_version: "1.0"\n'
            "domain: SE\n"
            "keys: [USUBJID]\n"
            "input: {}\n"
            f"output:\n  path: {name}.csv\n  columns: [{', '.join(artifact)}]\n"
            "columns:\n"
            + "".join(
                f"  - name: {column}\n    type: str\n    label: {column}\n"
                f"    derivation: {{literal: X}}\n"
                for column in declared
            )
        )
        (example / "expected" / f"{name}.csv").write_text(
            ",".join(header) + "\n" + ",".join("x" for _ in header) + "\n"
        )
        return Path(raw)

    def findings(self, name, declared, artifact, header):
        with tempfile.TemporaryDirectory() as raw:
            root = self.example(raw, name, declared, artifact, header)
            return OWNER_REPOSITORY_ARTIFACTS.validate_key_column_order(root)

    def test_accepts_ordered_key_columns(self):
        ordered = ["DOMAIN", "STUDYID", "USUBJID", "SESEQ"]
        self.assertEqual(
            self.findings("order-ok", ordered, ordered, ordered),
            [],
        )

    def test_accepts_a_missing_key_column(self):
        cols = ["DOMAIN", "USUBJID", "SESEQ"]
        self.assertEqual(self.findings("order-skip", cols, cols, cols), [])

    def test_rejects_a_swapped_declaration(self):
        declared = ["STUDYID", "DOMAIN", "USUBJID", "SESEQ"]
        ordered = ["DOMAIN", "STUDYID", "USUBJID", "SESEQ"]
        errors = self.findings("order-bad", declared, ordered, ordered)
        self.assertEqual(len(errors), 1)
        self.assertIn("order-bad/spec.yaml: declared columns", errors[0])
        self.assertIn("key columns out of order", errors[0])

    def test_rejects_a_swapped_golden_header(self):
        ordered = ["DOMAIN", "STUDYID", "USUBJID", "SESEQ"]
        header = ["STUDYID", "DOMAIN", "USUBJID", "SESEQ"]
        errors = self.findings("order-golden", ordered, ordered, header)
        self.assertEqual(len(errors), 1)
        self.assertIn("order-golden/expected/order-golden.csv: header", errors[0])


class TestSourceOrdinal(unittest.TestCase):
    """Validate generated source fields without changing stored field meaning."""

    def setUp(self):
        """Create a header-only source and its opt-in ordinal declaration."""
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.spec_path = self.root / "spec.yaml"
        self.spec = yaml.safe_load(
            (ROOT / "benchmarks/schema-source-ordinal/spec.yaml").read_text()
        )
        self.spec["input"]["SOURCE"]["path"] = "source.csv"
        (self.root / "source.csv").write_text("GroupID,RecordLabel,Include\n")

    def findings(self):
        """Return static contract findings for this test's source declaration."""
        return CONTROLS.contracts(self.spec, "spec.yaml", self.spec_path)

    def test_catalog_exposes_an_opt_in_integer(self):
        """The generated field is int and disappears without an opt-in."""
        self.assertEqual(self.findings(), [])
        catalog = SOURCE_CONTROLS.catalog(self.spec, self.spec_path)
        self.assertEqual(catalog["SOURCE"]["SourceOrdinal"], "int")
        del self.spec["input"]["SOURCE"]["ordinal"]
        catalog = SOURCE_CONTROLS.catalog(self.spec, self.spec_path)
        self.assertNotIn("SourceOrdinal", catalog["SOURCE"])

    def test_rejects_collision_with_a_header_only_source(self):
        """An empty source reserves its stored field names against ordinals."""
        (self.root / "source.csv").write_text(
            "GroupID,RecordLabel,Include,SourceOrdinal\n"
        )
        [finding] = self.findings()
        self.assertEqual(finding.condition, "ordinal_field_collision")
        self.assertEqual(finding.path, "spec.yaml.input.SOURCE.ordinal")
        self.assertEqual(
            finding.context,
            {
                "dataset": "SOURCE",
                "field": "SourceOrdinal",
            },
        )
        catalog = SOURCE_CONTROLS.catalog(self.spec, self.spec_path)
        self.assertEqual(catalog["SOURCE"]["SourceOrdinal"], "str")

    def test_rejects_a_type_for_the_generated_field(self):
        """Generated positions cannot be typed as if they were stored fields."""
        self.spec["input"]["SOURCE"]["types"] = {"SourceOrdinal": "int"}
        [finding] = self.findings()
        self.assertEqual(finding.condition, "unknown_field")

    def test_rejects_parquet_collision(self):
        """Parquet field names are checked before an ordinal is exposed."""
        import pyarrow as pa
        import pyarrow.parquet as pq

        pq.write_table(
            pa.table({"SourceOrdinal": pa.array([], pa.int64())}),
            self.root / "source.parquet",
        )
        self.spec["input"]["SOURCE"]["path"] = "source.parquet"
        [finding] = self.findings()
        self.assertEqual(finding.condition, "ordinal_field_collision")

    def test_rejects_an_ordinal_on_a_fixed_schema_odm_input(self):
        """ODM inputs cannot extend their fixed schema with an ordinal."""
        self.spec["columns"][0]["derivation"] = {"odm": "SOURCE.IT.DM.ID"}
        findings = SOURCE_CONTROLS.odm(self.spec, "spec.yaml", self.spec_path)
        [finding] = [
            item for item in findings if item.condition == "odm_schema_field_type"
        ]
        self.assertEqual(finding.path, "spec.yaml.input.SOURCE.ordinal")
        self.assertEqual(
            finding.context,
            {
                "dataset": "SOURCE",
                "declared": "ordinal",
            },
        )


if __name__ == "__main__":
    unittest.main()
