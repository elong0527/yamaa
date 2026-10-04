"""Installed normalized-spec frontend: native values and portable failure evidence."""

import copy
import datetime as dt
import itertools
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import polars as pl
import yamaa
import yamaa_native
from yamaa.adapters.native_datasets import (
    NativeDatasetLimitError,
    execute_with_source_provider,
)
from yamaa.adapters.observations import observe_scalar
from yamaa.io import ProjectResources, load_source_tables, render_artifact
from yamaa.io.polars import frame_from_values
from yamaa.models import TypedColumn
from yamaa.runtime import ExecutionHooks, ExecutionSuccess, execute_specification
from yamaa.specification import load_specification
from yamaa.verification import check_dataset

import yaml

ROOT = Path(__file__).parent
CASE = ROOT / "specification-adlb"
SCHEMA = ROOT / "specification-yaml"


def record_truth(records):
    """Retain private complete offending identities as well as public sampled context."""
    result = []
    for record in records:
        value = record.model_dump(mode="json")
        if record.failure is not None:
            value["failure"].update(
                offending_keys=list(record.failure.offending_keys),
                log_context=record.failure.log_context,
                severity=record.failure.severity,
            )
        result.append(value)
    return result


class InstalledSpecification(unittest.TestCase):
    """Exercise actual specification loading and installed Rust without reference fallback."""

    def setUp(self):
        """Retain the unchanged benchmark document and freshly ingested source table."""
        self.document = yaml.safe_load((CASE / "spec.yaml").read_text())
        self.spec = load_specification(CASE / "spec.yaml", SCHEMA).specification
        self.sources = load_source_tables(self.spec.input, ProjectResources(CASE))

    def test_committed_complete_window_benchmark(self):
        """Execute the complete unchanged window specification against its committed CSV."""
        case = ROOT / "specification-windows"
        spec = load_specification(case / "spec.yaml", SCHEMA).specification
        sources = load_source_tables(spec.input, ProjectResources(case))
        actual = self.compare(spec, sources)
        self.assertIsInstance(actual.result, ExecutionSuccess)
        expected = pl.read_csv(
            case / "expected/advs.csv",
            schema_overrides=actual.result.table.frame.schema,
        )
        self.assertTrue(actual.result.table.frame.equals(expected, null_equal=True))

    def test_baseline_temporal_candidates_filters_and_partition_conditions(self):
        """Compare per-row references, missing values, exact ties and conversion failures."""
        for kind, make in [("date", dt.date), ("datetime", dt.datetime)]:
            columns = tuple(
                TypedColumn(name=name, type=typ)
                for name, typ in [("ID", "int"), ("G", "str"), ("D", kind), ("R", kind)]
            )
            day = lambda n, make=make: make(2025, 1, n)
            ordinary = [
                [1, None, day(1), day(3)],
                [2, None, day(3), day(2)],
                [3, None, None, day(3)],
                [4, "b", day(2), None],
                [5, "b", day(2), day(2)],
                [6, "b", day(3), day(3)],
            ]
            tied = copy.deepcopy(ordinary)
            for row in tied[:3]:
                row[2:4] = [day(1), day(3)]
            for rows in [ordinary, tied, tied[:3], [], [[1, None, None, None]]]:
                for groups in [["G"], []]:
                    for predicate in [None, "ID > 2", "ID < 0"]:
                        for target in ["str", "int"]:
                            window = {"group_by": groups}
                            if predicate is not None:
                                window["filter"] = predicate
                            document = {
                                "schema_version": "1.0",
                                "domain": "BL",
                                "keys": ["ID"],
                                "input": {"SRC": "source.csv"},
                                "output": {
                                    "path": "out.csv",
                                    "columns": ["ID", "G", "D", "R", "BLFL"],
                                },
                                "columns": [
                                    {
                                        "name": c.name,
                                        "type": c.type,
                                        "label": c.name,
                                        "derivation": f"SRC.{c.name}",
                                    }
                                    for c in columns
                                ]
                                + [
                                    {
                                        "name": "BLFL",
                                        "type": target,
                                        "label": "Baseline",
                                        "derivation": {
                                            "baseline_flag": {
                                                "date": "D",
                                                "reference_date": "R",
                                                "window": window,
                                            }
                                        },
                                    }
                                ],
                            }
                            with self.subTest(
                                kind=kind,
                                rows=rows,
                                groups=groups,
                                predicate=predicate,
                                target=target,
                            ):
                                self.compare(
                                    self.load(document),
                                    {"SRC": frame_from_values(columns, rows)},
                                )

    def test_root_filter_precedes_all_keys_and_limits_feeding_records(self):
        """Root predicates finish first, discard unknown rows and retain exact group members."""
        columns = tuple(
            TypedColumn(name=n, type=t)
            for n, t in [("ID", "str"), ("V", "str"), ("KEEP", "int"), ("P", "str")]
        )
        rows = [
            ["bad", "bad", 0, "%"],
            ["02", "seven", 1, "!"],
            ["2", "conflict", None, "%"],
            ["1", "eight", 1, "%"],
            ["2", "seven", 1, "%"],
        ]
        for predicate in [
            "TRUE",
            "FALSE",
            "SRC.KEEP > 0",
            "SRC.V LIKE SRC.P ESCAPE '!'",
            "FALSE AND SRC.KEEP > 'bad'",
        ]:
            document = {
                "schema_version": "1.0",
                "domain": "ROOT",
                "keys": ["ID"],
                "input": {"SRC": "source.csv"},
                "base": "SRC",
                "filter": predicate,
                "output": {"path": "out.csv", "columns": ["ID", "V"]},
                "columns": [
                    {"name": n, "type": t, "label": n, "derivation": f"SRC.{n}"}
                    for n, t in [("ID", "int"), ("V", "str")]
                ],
            }
            for data in [rows, []]:
                with self.subTest(predicate=predicate, empty=not data):
                    actual = self.compare(
                        self.load(document), {"SRC": frame_from_values(columns, data)}
                    )
                    if predicate == "SRC.KEEP > 0" and data:
                        self.assertEqual(
                            actual.result.table.frame.rows(),
                            [(2, "seven"), (1, "eight")],
                        )

    def test_source_filters_select_feeders_without_removing_output_rows(self):
        """Source eligibility follows keys, precedes value collection and retains failure context."""
        columns = tuple(
            TypedColumn(name=n, type=t)
            for n, t in [("ID", "str"), ("V", "str"), ("KEEP", "int")]
        )
        original = [["02", "7", 1], ["2", "8", 0], ["2", "7", 1], ["1", "9", None]]
        for scenario in [
            "ordinary",
            "none",
            "multiple",
            "predicate",
            "convert",
            "key_first",
            "root",
        ]:
            rows = copy.deepcopy(original)
            predicate = {
                "none": "FALSE",
                "multiple": "TRUE",
                "predicate": "SRC.KEEP > 'bad'",
                "key_first": "SRC.KEEP > 'bad'",
            }.get(scenario, "SRC.KEEP > 0")
            if scenario == "convert":
                rows[0][1] = rows[2][1] = "bad"
            if scenario == "key_first":
                rows[-1][0] = "bad"
            document = {
                "schema_version": "1.0",
                "domain": "SRCFILTER",
                "keys": ["ID"],
                "base": "SRC",
                "input": {"SRC": "source.csv"},
                "output": {"path": "out.csv", "columns": ["ID", "V"]},
                "columns": [
                    {
                        "name": "ID",
                        "type": "int",
                        "label": "ID",
                        "derivation": "SRC.ID",
                    },
                    {
                        "name": "V",
                        "type": "int",
                        "label": "V",
                        "derivation": {
                            "source": {"variable": "SRC.V", "filter": predicate}
                        },
                    },
                ],
            }
            if scenario == "root":
                document["filter"] = "SRC.V <> '7'"
            for data in [rows, []]:
                with self.subTest(scenario=scenario, empty=not data):
                    actual = self.compare(
                        self.load(document), {"SRC": frame_from_values(columns, data)}
                    )
                    if data and scenario in {"ordinary", "none", "root"}:
                        self.assertEqual(
                            actual.result.table.frame.rows(),
                            [(2, 7 if scenario == "ordinary" else None), (1, None)],
                        )

    def test_numbering_directions_ties_empty_and_conversion(self):
        """Compare authored numbering variants, preserving complete result and failure evidence."""
        case = ROOT / "specification-windows"
        base = yaml.safe_load((case / "spec.yaml").read_text())
        base["columns"] = [
            column
            for column in base["columns"]
            if column["name"] in {"USUBJID", "VISITN", "VSSTRESN", "SEVRANKC"}
        ]
        base["output"]["columns"] = [column["name"] for column in base["columns"]]
        for operation, kind in [
            ("row_number", "int"),
            ("rank", "str"),
            ("rank", "date"),
        ]:
            for descending, first in itertools.product([False, True], repeat=2):
                document = copy.deepcopy(base)
                document["columns"][-1].update(
                    type=kind,
                    derivation={
                        operation: {
                            "window": {
                                "group_by": [],
                                "order_by": [
                                    {
                                        "variable": "VSSTRESN",
                                        "direction": "desc" if descending else "asc",
                                        "nulls": "first" if first else "last",
                                    },
                                    "VISITN",
                                ],
                            }
                        }
                    },
                )
                spec = self.load(document)
                sources = load_source_tables(spec.input, ProjectResources(case))
                with self.subTest(
                    operation=operation, kind=kind, descending=descending, first=first
                ):
                    self.compare(spec, sources)
                    source = sources["VS"].table
                    empty = type(source)(
                        columns=source.columns, frame=source.frame.clear()
                    )
                    self.compare(spec, {"VS": empty})

    def test_window_filter_truth_eagerness_empty_and_conversion_priority(self):
        """Partition predicates execute before current conversion and after earlier partitions."""
        document = {
            "schema_version": "1.0",
            "domain": "WIN",
            "keys": ["ID"],
            "input": {"SRC": "source.csv"},
            "output": {"path": "out.csv", "columns": ["ID", "G", "V", "N"]},
            "columns": [
                {"name": name, "type": kind, "label": name, "derivation": f"SRC.{name}"}
                for name, kind in [("ID", "int"), ("G", "str"), ("V", "int")]
            ]
            + [
                {
                    "name": "N",
                    "type": "int",
                    "label": "Number",
                    "derivation": {
                        "row_number": {
                            "window": {
                                "group_by": ["G"],
                                "order_by": [{"variable": "V", "nulls": "first"}],
                                "filter": "V > 0",
                            }
                        }
                    },
                }
            ],
        }
        columns = tuple(
            TypedColumn(name=name, type=kind)
            for name, kind in [("ID", "int"), ("G", "str"), ("V", "int")]
        )
        for kind in ["int", "date"]:
            for predicate in [
                "V > 0",
                "FALSE",
                "V IS NULL OR V > 'x'",
                "FALSE AND V > 'x'",
            ]:
                for same_group in [False, True]:
                    doc = copy.deepcopy(document)
                    doc["columns"][-1]["type"] = kind
                    doc["columns"][-1]["derivation"]["row_number"]["window"][
                        "filter"
                    ] = predicate
                    spec = self.load(doc)
                    source = frame_from_values(
                        columns,
                        [
                            [1, "a", None],
                            [2, "a" if same_group else "b", 1],
                            [3, "a" if same_group else "c", -1],
                        ],
                    )
                    with self.subTest(
                        kind=kind, predicate=predicate, same_group=same_group
                    ):
                        self.compare(spec, {"SRC": source})
                        self.compare(spec, {"SRC": frame_from_values(columns, [])})

    def test_window_value_types_offsets_filters_and_result_conversion(self):
        """Read completed donors with exact types, eligible offsets and ordinary conversion."""
        for kind, values in [
            ("str", [None, "\U0001f331", None, "z", None]),
            ("int", [None, 9007199254740993, None, 9223372036854775807, None]),
            ("float", [None, -0.0, None, 2.5, None]),
            ("date", [None, dt.date(2024, 1, 1), None, dt.date(2025, 1, 1), None]),
        ]:
            columns = tuple(
                TypedColumn(name=name, type=column_kind)
                for name, column_kind in [("ID", "int"), ("V", kind)]
            )
            source = frame_from_values(
                columns, [[i + 1, value] for i, value in enumerate(values)]
            )
            for operation, offset in [
                ("row_value", -1),
                ("row_value", 1),
                ("row_value", -9223372036854775808),
                ("row_value", 9223372036854775807),
                ("previous_non_missing", None),
                ("locf", None),
            ]:
                for filtered in [False, True]:
                    payload = {"source": "V", "window": {"order_by": ["ID"]}}
                    if offset is not None:
                        payload["offset"] = offset
                    if filtered:
                        payload["window"]["filter"] = "ID <> 2"
                    document = {
                        "schema_version": "1.0",
                        "domain": "WIN",
                        "keys": ["ID"],
                        "input": {"SRC": "source.csv"},
                        "output": {"path": "out.csv", "columns": ["ID", "V", "N"]},
                        "columns": [
                            {
                                "name": "ID",
                                "type": "int",
                                "label": "Identity",
                                "derivation": "SRC.ID",
                            },
                            {
                                "name": "V",
                                "type": kind,
                                "label": "Value",
                                "derivation": "SRC.V",
                            },
                            {
                                "name": "N",
                                "type": kind,
                                "label": "Window",
                                "derivation": {operation: payload},
                            },
                        ],
                    }
                    with self.subTest(
                        kind=kind, operation=operation, offset=offset, filtered=filtered
                    ):
                        self.compare(self.load(document), {"SRC": source})
                        document["columns"][-1]["type"] = (
                            "int" if kind == "str" else "str"
                        )
                        self.compare(self.load(document), {"SRC": source})
                        self.compare(
                            self.load(document), {"SRC": frame_from_values(columns, [])}
                        )

    def load(self, document):
        """Validate authored test variants through the real schema and normalization pipeline."""
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "spec.yaml"
            path.write_text(yaml.safe_dump(document, sort_keys=False), encoding="utf-8")
            return load_specification(path, SCHEMA).specification

    def compare(self, spec, sources):
        """Compare native production with independently invoked reference observations."""
        records = []

        def dataset(table, declarations, keys, *, records=None):
            """Capture the reference's real checks only in the reference execution."""
            observed = []
            try:
                return check_dataset(table, declarations, keys, records=observed)
            finally:
                reference_records.extend(observed)
                if records is not None:
                    records.extend(observed)

        reference_records = records
        reference = execute_specification(
            spec, sources, hooks=ExecutionHooks(dataset=dataset)
        )
        effects = []

        def provider(declarations):
            """Prove one provider call and preserve declared sources without discovery."""
            effects.append(tuple(declarations))
            return sources

        with (
            patch(
                "yamaa.runtime.executor.execute_specification",
                side_effect=AssertionError("reference execution"),
            ),
            patch(
                "yamaa.runtime.lifecycle.ExpressionDispatcher.evaluate",
                side_effect=AssertionError("reference evaluation"),
            ),
            patch(
                "yamaa.runtime.executor.evaluate_predicate",
                side_effect=AssertionError("reference predicate evaluation"),
            ),
            patch(
                "yamaa.runtime.rows.RowResolver._window",
                side_effect=AssertionError("reference window evaluation"),
            ),
            patch(
                "yamaa.runtime.executor._key_space",
                side_effect=AssertionError("reference key construction"),
            ),
            patch(
                "yamaa.runtime.executor._key_grain_candidates",
                side_effect=AssertionError("reference key candidates"),
            ),
            patch(
                "yamaa.verification.checks.evaluate_predicate",
                side_effect=AssertionError("reference verification predicate"),
            ),
            patch(
                "yamaa.verification.checks.check_dataset",
                side_effect=AssertionError("reference checks"),
            ),
            patch(
                "yamaa.verification.checks.check_keys",
                side_effect=AssertionError("reference keys"),
            ),
        ):
            actual = execute_with_source_provider(spec, provider)
        self.assertEqual(effects, [tuple(spec.input)])
        self.assertEqual(actual.result.status, reference.status)
        self.assertEqual(actual.result.handler_counts, reference.handler_counts)
        self.assertEqual(record_truth(actual.verifications), record_truth(records))
        if isinstance(reference, ExecutionSuccess):
            self.assertEqual(actual.result.table.columns, reference.table.columns)
            self.assertTrue(
                actual.result.table.frame.equals(reference.table.frame, null_equal=True)
            )
            self.assertEqual(
                [
                    [observe_scalar(value) for value in row]
                    for row in actual.result.table.frame.iter_rows()
                ],
                [
                    [observe_scalar(value) for value in row]
                    for row in reference.table.frame.iter_rows()
                ],
            )
            self.assertEqual(
                render_artifact(actual.result.artifact),
                render_artifact(reference.artifact),
            )
            if reference.warning_log is not None:
                self.assertEqual(
                    render_artifact(actual.result.warning_log),
                    render_artifact(reference.warning_log),
                )
        else:
            self.assertEqual(actual.result.diagnostics, reference.diagnostics)
            self.assertFalse(hasattr(actual.result, "artifact"))
        if reference.verification_log is not None:
            self.assertEqual(
                render_artifact(actual.result.verification_log),
                render_artifact(reference.verification_log),
            )
        return actual

    def test_real_adlb_matches_committed_truth(self):
        """Run the actual YAML/input through installed code and match unchanged CSV bytes."""
        self.assertIn("site-packages", str(Path(yamaa.__file__).resolve()))
        self.assertIn("site-packages", str(Path(yamaa_native.__file__).resolve()))
        actual = self.compare(self.spec, self.sources)
        self.assertEqual(
            render_artifact(actual.result.artifact),
            (CASE / "expected/adlb.csv").read_bytes(),
        )
        self.assertEqual(
            [record.evaluated_count for record in actual.verifications], [17, 1]
        )
        self.assertFalse(yamaa_native.engine_info()["execution_supported"])

    def test_row_filters_match_committed_subset_and_reference(self):
        """Record and grouped filters preserve completed values and original order."""
        doc = copy.deepcopy(self.document)
        doc["rows"][0]["filter"] = "AVAL > 0 AND LB.LBTESTCD LIKE 'COMP%'"
        doc["rows"][1]["filter"] = "AVAL > 0.5"
        doc.pop("verifications")
        actual = self.compare(self.load(doc), self.sources)
        # Independently select the literal row ordinals from the committed CSV.
        expected = (CASE / "expected/adlb.csv").read_bytes().splitlines(keepends=True)
        self.assertEqual(
            render_artifact(actual.result.artifact),
            b"".join(
                expected[index] for index in [0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 13, 14]
            ),
        )

    def test_filter_operator_paths_and_failures(self):
        """Every admitted AST family executes natively, including eager failure paths."""
        for expression in (
            "TRUE",
            "FALSE",
            "NOT (AVAL IS NULL)",
            "AVAL IS NULL",
            "AVAL IS NOT NULL",
            "AVAL IN (0, 1, NULL)",
            "AVAL NOT IN (0, 1)",
            "AVAL BETWEEN 0 AND 10",
            "AVAL NOT BETWEEN 0 AND 10",
            "LB.LBTESTCD LIKE 'COMP_'",
            "LB.LBTESTCD NOT LIKE '%Z%'",
            "LB.LBTESTCD LIKE 'COMP!_%' ESCAPE '!'",
            "AVAL <= 0 OR AVAL >= 1",
            "DATE '2024-01-01' = DATE '2024-01-01'",
            "DATETIME '2024-01-01T00:00' = DATETIME '2024-01-01T00:00:00'",
            "AVAL < 1e999",
            "9007199254740993 = 9007199254740992.0",
            "NOT (AVAL <> 1)",
            "FALSE AND AVAL = 'bad'",
            "TRUE OR AVAL = 'bad'",
            "AVAL LIKE '%'",
            "AVAL IN (1, 'bad')",
            "AVAL BETWEEN 0 AND 'bad'",
        ):
            with self.subTest(expression=expression):
                doc = copy.deepcopy(self.document)
                doc["rows"][0]["filter"] = expression
                doc.pop("verifications")
                self.compare(self.load(doc), self.sources)

    def test_false_filter_does_not_hide_row_conversion(self):
        """Conversion is completed before filtering even for an always-false predicate."""
        doc = copy.deepcopy(self.document)
        doc["rows"][0]["filter"] = "FALSE"
        doc["rows"][0]["derivations"]["AVAL"] = {"literal": True}
        self.assertEqual(
            self.compare(self.load(doc), self.sources).result.status, "failure"
        )

    def test_assertion_and_implication_records(self):
        """All completed check observations and exact failed identities survive native checks."""
        for held in (False, True):
            with self.subTest(held=held):
                doc = copy.deepcopy(self.document)
                doc["output"]["verification_log"] = "checks.csv"
                doc["verifications"].extend(
                    [
                        {
                            "assert": {
                                "id": "avals",
                                "expr": "AVAL IS NULL OR AVAL >= 0"
                                if held
                                else "AVAL IS NOT NULL",
                            }
                        },
                        {
                            "implies": {
                                "id": "derived",
                                "when": "DTYPE IS NOT NULL"
                                if held
                                else "DTYPE IS NULL",
                                "then": "PARAMCD = 'TOTAL'"
                                if held
                                else "AVAL IS NOT NULL",
                            }
                        },
                    ]
                )
                actual = self.compare(self.load(doc), self.sources)
                self.assertEqual(actual.result.status, "success" if held else "failure")
                self.assertEqual(
                    [record.evaluated_count for record in actual.verifications],
                    [17, 1, 17, 17],
                )
                if not held:
                    self.assertEqual(
                        [
                            record.failure.context["failure_count"]
                            for record in actual.verifications[2:]
                        ],
                        [4, 3],
                    )

    def test_predicate_check_conditions_and_declaration_order(self):
        """Declaration and dynamic errors preserve exact diagnostics and prior ledger rows."""
        cases = [
            {"assert": {"expr": "AVAL = 'bad'"}},
            {"assert": {"expr": "z = a"}},
            {"assert": {"expr": "AVAL >"}},
            {"implies": {"when": "FALSE", "then": "AVAL = 'bad'"}},
            {"implies": {"when": "FALSE", "then": "'x' LIKE PARAMCD ESCAPE '1'"}},
            {"implies": {"when": "AVAL = 'bad'", "then": "AVAL >"}},
            {"implies": {"when": "AVAL = 'bad'", "then": "z = a"}},
            {"implies": {"when": "'x' LIKE PARAMCD ESCAPE '1'", "then": "AVAL >"}},
        ]
        for case in cases:
            for empty in (False, True):
                with self.subTest(case=case, empty=empty):
                    doc = copy.deepcopy(self.document)
                    doc["output"]["verification_log"] = "checks.csv"
                    doc["verifications"][1]["row_count"] = {"min": 18, "max": 18}
                    doc["verifications"].append(case)
                    table = self.sources["LB"].table
                    if empty:
                        table = table.model_copy(update={"frame": table.frame.head(0)})
                    self.compare(self.load(doc), {"LB": table})

    def test_check_predicates_follow_key_and_conversion_failures(self):
        """Native sample evaluation cannot move a verification condition before data gates."""
        for scenario in ("keys", "conversion"):
            with self.subTest(scenario=scenario):
                doc = copy.deepcopy(self.document)
                doc["output"]["verification_log"] = "checks.csv"
                doc["verifications"].append({"assert": {"expr": "AVAL = 'bad'"}})
                if scenario == "keys":
                    doc["keys"] = ["STUDYID"]
                else:
                    doc["rows"][0]["derivations"]["AVAL"] = {"literal": True}
                self.compare(self.load(doc), self.sources)

    def test_failures_and_complete_logs(self):
        """Failed checks retain all records, sampled diagnostics and complete private keys."""
        for scenario in (
            "held",
            "row_count",
            "unique",
            "duplicate",
            "missing",
            "conversion",
            "overflow",
        ):
            with self.subTest(scenario=scenario):
                doc = copy.deepcopy(self.document)
                doc["output"]["verification_log"] = "verification.csv"
                doc["output"]["warning_log"] = "warnings.csv"
                doc["verifications"][0]["unique"]["id"] = "identity-check"
                table = self.sources["LB"].table
                if scenario == "row_count":
                    doc["verifications"][1]["row_count"] = {
                        "min": 18,
                        "max": 18,
                        "id": "count-check",
                    }
                elif scenario == "unique":
                    doc["verifications"][0]["unique"]["columns"] = ["STUDYID"]
                elif scenario == "duplicate":
                    doc["keys"] = ["STUDYID"]
                elif scenario == "missing":
                    table = table.model_copy(
                        update={
                            "frame": table.frame.with_columns(
                                pl.lit(None, dtype=pl.String).alias("STUDYID")
                            )
                        }
                    )
                elif scenario == "conversion":
                    doc["rows"][0]["derivations"]["AVAL"] = {"literal": True}
                elif scenario == "overflow":
                    doc["input"]["LB"]["types"]["LBSTRESN"] = "int"
                    columns = tuple(
                        TypedColumn(
                            name=c.name, type="int" if c.name == "LBSTRESN" else c.type
                        )
                        for c in table.columns
                    )
                    table = frame_from_values(
                        columns,
                        [
                            list(row[:-1]) + [2**63 - 1]
                            for row in table.frame.iter_rows()
                        ],
                    )
                actual = self.compare(self.load(doc), {"LB": table})
                self.assertEqual(
                    actual.result.status, "success" if scenario == "held" else "failure"
                )

    def test_empty_templates_and_completed_missing_keys(self):
        """Literal conversion timing and identity completeness survive the real frontend."""
        for count, row_phase, missing, grouped in itertools.product(
            (0, 1), (False, True), (False, True), (False, True)
        ):
            with self.subTest(
                count=count, row_phase=row_phase, missing=missing, grouped=grouped
            ):
                doc = {
                    "schema_version": "1.0",
                    "domain": "TEST",
                    "keys": ["id"],
                    "input": {"T": {"path": "input.csv"}},
                    "output": {
                        "path": "output.csv",
                        "columns": ["id", "value"],
                        "verification_log": "checks.csv",
                    },
                    "columns": [
                        {"name": "id", "type": "str", "label": "ID"},
                        {"name": "value", "type": "float", "label": "Value"},
                    ],
                    "rows": [{"id": "r", "derivations": {}}],
                }
                if row_phase:
                    doc["rows"][0]["derivations"]["value"] = {"literal": True}
                    doc["columns"][0]["derivation"] = "T.id"
                else:
                    doc["rows"][0]["derivations"]["id"] = "T.id"
                    doc["columns"][1]["derivation"] = {"literal": True}
                if grouped:
                    doc["rows"][0]["group_by"] = ["T.id"]
                table = frame_from_values(
                    (TypedColumn(name="id", type="str"),),
                    [[None if missing else "a"]] * count,
                )
                self.compare(self.load(doc), {"T": table})

    def test_invalid_later_declarations_retain_check_prefix(self):
        """Invalid declarations retain prior check records while key/derivation failures win."""
        for scenario in (
            "first",
            "after_pass",
            "after_fail",
            "after_keys",
            "after_conversion",
            "duplicate_id",
        ):
            with self.subTest(scenario=scenario):
                doc = copy.deepcopy(self.document)
                doc["output"]["verification_log"] = "verification.csv"
                bad = {"unique": {"columns": ["NOPE"]}}
                if scenario == "first":
                    doc["verifications"].insert(0, bad)
                else:
                    doc["verifications"].append(bad)
                if scenario == "after_fail":
                    doc["verifications"][0]["unique"]["columns"] = ["STUDYID"]
                elif scenario == "after_keys":
                    doc["keys"] = ["STUDYID"]
                elif scenario == "after_conversion":
                    doc["rows"][0]["derivations"]["AVAL"] = {"literal": True}
                elif scenario == "duplicate_id":
                    doc["verifications"][0]["unique"]["id"] = "repeated"
                    doc["verifications"][-1] = {
                        "unique": {"columns": ["STUDYID"], "id": "repeated"}
                    }
                self.compare(self.load(doc), self.sources)

    def test_defaults_projection_order_and_decimals(self):
        """Resolved template defaults and host artifact formatting retain reference behavior."""
        doc = copy.deepcopy(self.document)
        doc["rows"][1]["derivations"].pop("DTYPE")
        doc["columns"][6]["derivation"] = {"literal": "DEFAULT"}
        doc["output"]["columns"] = list(reversed(doc["output"]["columns"]))
        doc["output"]["decimals"] = 4
        doc["output"]["order_by"] = [{"variable": "USUBJID", "direction": "desc"}]
        self.compare(self.load(doc), self.sources)

    def test_source_ordinals(self):
        """The existing ingestion port assigns ordinals which native rows read losslessly."""
        doc = copy.deepcopy(self.document)
        doc["input"]["LB"]["ordinal"] = "RECNO"
        doc["columns"].append({"name": "ORD", "type": "int", "label": "Ordinal"})
        doc["output"]["columns"].append("ORD")
        doc["rows"][0]["derivations"]["ORD"] = "LB.RECNO"
        doc["rows"][1]["derivations"]["ORD"] = {"literal": 0}
        spec = self.load(doc)
        sources = load_source_tables(spec.input, ProjectResources(CASE))
        actual = self.compare(spec, sources)
        self.assertEqual(
            actual.result.table.frame["ORD"].to_list(), list(range(1, 13)) + [0] * 5
        )

    def test_admitted_specification_is_owned_across_provider_effects(self):
        """Caller/provider mutations cannot replace the already admitted expressions."""
        spec = self.spec

        def provider(declarations):
            """Mutate both caller-owned model internals and the supplied IO declarations."""
            spec.rows[0].derivations["AVAL"].value.root.clear()
            spec.rows[0].derivations["AVAL"].value.root["compute"] = "1 / 0"
            declarations.clear()
            return self.sources

        actual = execute_with_source_provider(spec, provider)
        self.assertEqual(actual.result.status, "success")
        self.assertEqual(
            render_artifact(actual.result.artifact),
            (CASE / "expected/adlb.csv").read_bytes(),
        )

    def test_native_limit_is_not_a_semantic_result(self):
        """Resource refusal propagates once without an accepted artifact or fallback."""
        doc = copy.deepcopy(self.document)
        doc["rows"][0]["derivations"]["DTYPE"] = {"literal": "x" * 100000}
        spec = self.load(doc)
        effects = []

        def provider(_):
            """Count the only provider call made before bounded native execution."""
            effects.append("read")
            return self.sources

        with self.assertRaises(NativeDatasetLimitError) as raised:
            execute_with_source_provider(spec, provider)
        self.assertEqual(raised.exception.resource, "output_text_bytes")
        self.assertEqual(effects, ["read"])
        self.assertEqual(
            execute_with_source_provider(
                self.spec, lambda _: self.sources
            ).result.status,
            "success",
        )

    def test_temporal_host_storage_and_missingness(self):
        """Civil extrema and null temporal parents retain values through both host boundaries."""
        for kind, values in (
            ("date", [dt.date(1, 1, 1), None, dt.date(9999, 12, 31)]),
            (
                "datetime",
                [dt.datetime(1, 1, 1), None, dt.datetime(9999, 12, 31, 23, 59, 59)],  # noqa: DTZ001 - zone-free civil time
            ),
        ):
            with self.subTest(kind=kind):
                doc = {
                    "schema_version": "1.0",
                    "domain": "TEST",
                    "keys": ["id"],
                    "input": {"T": {"path": "input.csv", "types": {"D": kind}}},
                    "output": {"path": "out.csv", "columns": ["id", "D"]},
                    "columns": [
                        {
                            "name": "id",
                            "type": "int",
                            "label": "ID",
                            "derivation": "T.id",
                        },
                        {"name": "D", "type": kind, "label": "Date"},
                    ],
                    "rows": [{"id": "r", "derivations": {"D": "T.D"}}],
                }
                columns = (
                    TypedColumn(name="id", type="int"),
                    TypedColumn(name="D", type=kind),
                )
                table = frame_from_values(
                    columns, [[index, value] for index, value in enumerate(values)]
                )
                self.compare(self.load(doc), {"T": table})

    def key_document(self):
        """Author a current-schema key-grain plan with identity order unlike column order."""
        return {
            "schema_version": "1.0",
            "domain": "KEYS",
            "keys": ["TAG", "ID"],
            "input": {"SRC": "input.csv"},
            "output": {
                "path": "out.csv",
                "columns": ["ID", "TAG", "VALUE"],
                "verification_log": "checks.csv",
            },
            "columns": [
                {
                    "name": "ID",
                    "type": "int",
                    "label": "Identity",
                    "derivation": "SRC.id",
                },
                {"name": "TAG", "type": "str", "label": "Tag", "derivation": "SRC.tag"},
                {
                    "name": "VALUE",
                    "type": "int",
                    "label": "Value",
                    "derivation": "SRC.value",
                },
            ],
            "verifications": [{"unique": {"columns": ["TAG", "ID"]}}],
        }

    def key_source(self, rows):
        """Supply literal independent readings without asking either engine for truth."""
        columns = tuple(
            TypedColumn(name=name, type="str") for name in ("id", "tag", "value")
        )
        return {"SRC": frame_from_values(columns, rows)}

    def test_key_grain_values_order_empty_and_dependent_keys(self):
        """Converted identities and all feeding records compose through the installed engine."""
        rows = [
            ["02", "__missing_key__", None],
            ["01", "b", "7"],
            ["2", "__missing_key__", "9"],
            ["2", "__missing_key__", "9"],
        ]
        for empty, dependent in itertools.product((False, True), repeat=2):
            with self.subTest(empty=empty, dependent=dependent):
                doc = self.key_document()
                if dependent:
                    doc["columns"][1]["derivation"] = "ID"
                    doc["rows"] = []
                actual = self.compare(
                    self.load(doc), self.key_source([] if empty else rows)
                )
                expected = (
                    []
                    if empty
                    else [
                        (2, "2" if dependent else "__missing_key__", 9),
                        (1, "1" if dependent else "b", 7),
                    ]
                )
                self.assertEqual(actual.result.table.frame.rows(), expected)
        actual = self.compare(
            self.load(self.key_document()),
            self.key_source([["1", "a", None], ["1", "a", None]]),
        )
        self.assertEqual(actual.result.table.frame.rows(), [(1, "a", None)])

    def test_key_grain_conflicts_and_missing_tokens_preserve_diagnostics(self):
        """Raw conflict counts, missing identities and prior conversion ordering match exactly."""
        cases = [
            ([["1", "a", "07"], ["1", "a", "7"]], "multiple_values_per_key"),
            (
                [["1", "a", "7"], ["1", "a", "8"], ["1", "a", "9"], ["1", "a", "8"]],
                "multiple_values_per_key",
            ),
            ([["1", "__missing_key__", "7"], [None, "present", "8"]], "missing_key"),
            ([[None, "present", "8"], ["0", "__missing_key__", "7"]], "missing_key"),
            ([[None, "present", "8"], [None, "present", "9"]], "missing_key"),
            ([["1", "a", "bad value"], ["bad key", "b", "7"]], "conversion_failed"),
        ]
        for rows, condition in cases:
            with self.subTest(rows=rows):
                actual = self.compare(
                    self.load(self.key_document()), self.key_source(rows)
                )
                self.assertEqual(actual.result.diagnostics[0].condition, condition)

    def test_key_grain_keeps_literal_and_verification_phase_boundaries(self):
        """Empty keys skip per-value conversion but still validate dataset predicates."""
        for empty in (False, True):
            for invalid_key in (False, True):
                with self.subTest(empty=empty, invalid_key=invalid_key):
                    doc = self.key_document()
                    if invalid_key:
                        doc["columns"][0]["derivation"] = {"literal": True}
                    else:
                        doc["verifications"].append(
                            {"assert": {"expr": "VALUE = 'bad'"}}
                        )
                    self.compare(
                        self.load(doc),
                        self.key_source([] if empty else [["1", "a", "7"]]),
                    )


if __name__ == "__main__":
    unittest.main()
