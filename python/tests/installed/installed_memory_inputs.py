"""Owned Arrow input overrides from the shared memory-inputs truth (#4086)."""

import datetime
import json
import tempfile
import unittest
from pathlib import Path

import pandas as pd
import polars as pl
import pyarrow as pa
from host_fixtures import load as load_truth

import yamaa

TRUTH = load_truth(__file__, "memory-inputs")
KINDS = ("arrow", "polars", "pandas", "pandas-subclass", "protocol")


class PandasSubclass(pd.DataFrame):
    """A pandas frame remains index-free when its concrete class is external."""


class ArrowProvider:
    """A protocol-only object, independent of a library's Python frame type."""

    def __init__(self, table):
        self.table = table

    def __arrow_c_stream__(self, requested_schema=None):
        return self.table.__arrow_c_stream__(requested_schema)


def frame(case, kind):
    columns = dict(zip(TRUTH["columns"], zip(*case["rows"])))
    if kind in {"pandas", "pandas-subclass"}:
        cls = PandasSubclass if kind == "pandas-subclass" else pd.DataFrame
        result = cls(columns)
        # A pandas index is not an input column.
        # ABSENT is deliberately declared in the missing-type control. Its
        # index values must not satisfy a study-column declaration.
        result.index = pd.Index(list(range(100, 100 + len(result))), name="ABSENT")
        return result
    table = pa.table(columns)
    if kind == "polars":
        return pl.from_arrow(table)
    return ArrowProvider(table) if kind == "protocol" else table


def temporal_columns(rows):
    """The temporal-unit rows as the Python values a caller would hold."""
    identifiers, datetimes, times = zip(*rows)
    return {
        "ID": list(identifiers),
        "WHEN": [
            None if value is None else datetime.datetime.fromisoformat(value)
            for value in datetimes
        ],
        "AT": [
            None if value is None else datetime.time.fromisoformat(value)
            for value in times
        ],
    }


class MemoryInputs(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="yamaa-memory-inputs-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        for name, text in TRUTH["files"].items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding="utf-8")
        self.spec = yamaa.spec(self.root / TRUTH["entry"])

    def assert_saved(self, run, expected):
        self.assertEqual((run.status, run.issues.to_pylist()), ("succeeded", []))
        run.save()
        self.assertEqual((self.root / "out.csv").read_text(encoding="utf-8"), expected)

    def assert_memory(self, run):
        row = next(
            row
            for row in run.report.to_pylist()
            if row["KIND"] == "input" and row["NAME"] == TRUTH["alias"]
        )
        self.assertEqual(json.loads(row["DETAILS"]), TRUTH["memory_details"])

    def assert_issue(self, rows, expected):
        # The shared complete semantic record; entry/source coordinates are
        # separately covered by the installed issue-location family.
        observed = []
        for row in rows:
            observed.append(
                {
                    key: json.loads(row[key])
                    if key == "context"
                    else row["spec_paths"]
                    if key == "spec_path"
                    else row[key]
                    for key in expected
                    if key not in {"alias", "declaration_from", "declaration_to"}
                }
            )
        wanted = {
            key: value
            for key, value in expected.items()
            if key not in {"alias", "declaration_from", "declaration_to"}
        }
        self.assertEqual(observed, [wanted])

    def test_library_frames_and_protocol_objects_override_inherited_aliases(self):
        for kind in KINDS:
            for case in TRUTH["cases"]:
                with self.subTest(kind=kind, case=case["name"]):
                    original = frame(case, kind)
                    run = self.spec.run(inputs={TRUTH["alias"]: original})
                    self.assert_saved(run, case["csv"])
                    self.assert_memory(run)
            self.assert_saved(self.spec.run(), TRUTH["file_csv"])

    def test_aliases_sharing_a_missing_file_are_independent(self):
        (self.root / "data.csv").unlink()
        run = self.spec.run(
            inputs={
                "SRC": frame(TRUTH["cases"][0], "arrow"),
                "AUX": frame(TRUTH["cases"][1], "arrow"),
            }
        )
        self.assert_saved(run, TRUTH["all_memory_csv"])

    def test_a_provider_cannot_change_the_input_entries_being_imported(self):
        # Python providers can mutate their caller's dictionary during export.
        (self.root / "data.csv").unlink()
        inputs = {}

        class MutatingProvider(ArrowProvider):
            def __arrow_c_stream__(self, requested_schema=None):
                inputs.clear()
                return super().__arrow_c_stream__(requested_schema)

        inputs.update(
            SRC=MutatingProvider(frame(TRUTH["cases"][0], "arrow")),
            AUX=frame(TRUTH["cases"][1], "arrow"),
        )
        self.assert_saved(self.spec.run(inputs=inputs), TRUTH["all_memory_csv"])

    def test_a_deferred_run_keeps_owned_values_after_the_frame_changes(self):
        case = TRUTH["cases"][0]
        original = frame(case, "pandas")
        run = self.spec.run(stream=True, inputs={"SRC": original})
        original.iloc[0, 1] = TRUTH["cases"][1]["rows"][0][1]
        # Reading drives the deferred run after the caller's frame changes.
        self.assertEqual(pa.table(run.output).to_pylist(), case["output_rows"])
        self.assert_saved(run, case["csv"])
        self.assert_memory(run)

    def test_unknown_alias_fails_before_any_file_input_is_inspected(self):
        (self.root / "data.csv").unlink()
        run = self.spec.run(
            inputs={TRUTH["unknown"]["alias"]: frame(TRUTH["cases"][0], "arrow")}
        )
        self.assertEqual(run.status, "failed")
        self.assert_issue(run.issues.to_pylist(), TRUTH["unknown"])

    def test_declared_types_are_admitted_against_the_frame_schema(self):
        expected = TRUTH["missing_type"]
        path = self.root / "parent.yaml"
        path.write_text(
            path.read_text(encoding="utf-8").replace(
                expected["declaration_from"], expected["declaration_to"]
            ),
            encoding="utf-8",
        )
        for kind in KINDS:
            with self.subTest(kind=kind):
                run = self.spec.run(inputs={"SRC": frame(TRUTH["cases"][0], kind)})
                self.assert_issue(run.issues.to_pylist(), expected)

    def test_unsupported_physical_types_name_their_column_and_arrow_type(self):
        arrays = {
            "bool": pa.array([True]),
            "int32": pa.array([42], type=pa.int32()),
            # A zone is refused in every unit; a zone-free timestamp is read.
            "timestamp[us, tz=UTC]": pa.array([0], type=pa.timestamp("us", tz="UTC")),
        }
        for case in TRUTH["unsupported_types"]:
            with self.subTest(case=case["name"]):
                value = pa.table(
                    {
                        "ID": pa.array([1], type=pa.int64()),
                        "VALUE": arrays[case["type"]],
                    }
                )
                with self.assertRaises(yamaa.Error) as caught:
                    self.spec.run(inputs={"SRC": value})
                self.assert_issue(caught.exception.issues.to_pylist(), case["issue"])
                self.assertEqual(caught.exception.paths, [])

    def test_a_pandas_frame_is_typed_by_the_input_declaration(self):
        truth = TRUTH["frame_types"]
        path = self.root / "frame-types.yaml"
        path.write_text(truth["specification"], encoding="utf-8")
        # Each kind as pandas holds it: an integer column with a missing value
        # is float64, and a column of None is an object column that PyArrow
        # reads as the null type.
        dtypes = {"int": "int64", "float": "float64", "str": object, "missing": object}
        for case in truth["cases"]:
            with self.subTest(case=case["name"]):
                frame = pd.DataFrame(
                    {
                        column["name"]: pd.Series(
                            column["values"], dtype=dtypes[column["kind"]]
                        )
                        for column in case["columns"]
                    }
                )
                run = yamaa.spec(path).run(inputs={"SRC": frame})
                if "issue" in case:
                    self.assertEqual(run.status, "failed")
                    self.assert_issue(run.issues.to_pylist(), case["issue"])
                    continue
                self.assertEqual(
                    (run.status, run.issues.to_pylist()), ("succeeded", [])
                )
                run.save()
                self.assertEqual(
                    (self.root / "frame-types.csv").read_text(encoding="utf-8"),
                    case["csv"],
                )

    def test_protocol_errors_keep_the_original_cause(self):
        # Host protocol exceptions cannot be represented by shared frame data.
        original = ValueError("provider failed")

        class FailingProvider:
            def __arrow_c_stream__(self, requested_schema=None):
                raise original

        with self.assertRaises(yamaa.Error) as caught:
            self.spec.run(inputs={"SRC": FailingProvider()})
        self.assertEqual(
            (
                type(caught.exception),
                caught.exception.issues.to_pylist(),
                caught.exception.paths,
            ),
            (yamaa.Error, [], []),
        )
        self.assertIs(caught.exception.__cause__, original)

    def test_provider_control_flow_exceptions_escape_unchanged(self):
        # Provider control flow must cross the host boundary by identity.
        for original in (
            KeyboardInterrupt(),
            SystemExit(7),
            GeneratorExit(),
            BaseException(),
        ):
            with self.subTest(kind=type(original).__name__):

                class InterruptingProvider:
                    def __arrow_c_stream__(self, requested_schema=None):
                        raise original

                with self.assertRaises(type(original)) as caught:
                    self.spec.run(inputs={"SRC": InterruptingProvider()})
                self.assertIs(caught.exception, original)

    def test_all_admitted_scalar_kinds_and_nulls_share_the_authored_csv(self):
        case = TRUTH["typed"]
        arrays = {}
        for column in case["columns"]:
            kind = column["kind"]
            values = column["values"]
            if kind in {"date", "datetime"}:
                parser = (
                    datetime.date.fromisoformat
                    if kind == "date"
                    else datetime.datetime.fromisoformat
                )
                values = [None if value is None else parser(value) for value in values]
            arrow_type = {
                "int": pa.int64(),
                "str": pa.string(),
                "float": pa.float64(),
                "date": pa.date32(),
                "datetime": pa.timestamp("s"),
            }[kind]
            arrays[column["name"]] = pa.array(values, type=arrow_type)
        path = self.root / "typed.yaml"
        path.write_text(case["specification"], encoding="utf-8")
        run = yamaa.spec(path).run(inputs={"SRC": pa.table(arrays)})
        self.assertEqual((run.status, run.issues.to_pylist()), ("succeeded", []))
        run.save()
        self.assertEqual(
            (self.root / "typed.csv").read_text(encoding="utf-8"), case["csv"]
        )

    def test_a_datetime_and_a_time_in_any_unit_share_the_authored_csv(self):
        truth = TRUTH["temporal_units"]
        path = self.root / "units.yaml"
        path.write_text(truth["specification"], encoding="utf-8")
        columns = temporal_columns(truth["rows"])
        frames = {
            f"{unit['datetime']}, {unit['time']}": pa.table(
                {
                    "ID": pa.array(columns["ID"], type=pa.int64()),
                    "WHEN": pa.array(
                        columns["WHEN"], type=pa.type_for_alias(unit["datetime"])
                    ),
                    "AT": pa.array(columns["AT"], type=pa.type_for_alias(unit["time"])),
                }
            )
            for unit in truth["units"]
        }
        # The units each library gives these values when the caller names none.
        frames["pandas"] = pd.DataFrame(columns)
        frames["pandas ArrowDtype"] = pa.table(columns).to_pandas(
            types_mapper=pd.ArrowDtype
        )
        frames["polars"] = pl.DataFrame(columns)
        for label, original in frames.items():
            with self.subTest(frame=label):
                run = yamaa.spec(path).run(inputs={"SRC": original})
                self.assertEqual(
                    (run.status, run.issues.to_pylist()), ("succeeded", [])
                )
                run.save()
                saved = self.root / "units.csv"
                self.assertEqual(saved.read_text(encoding="utf-8"), truth["csv"])
                saved.unlink()

    def test_a_fraction_of_a_second_has_an_authored_input_diagnostic(self):
        truth = TRUTH["temporal_units"]
        path = self.root / "units.yaml"
        path.write_text(truth["specification"], encoding="utf-8")
        parsers = {"WHEN": datetime.datetime, "AT": datetime.time}
        for case in truth["fractions"]:
            with self.subTest(case=case["name"]):
                columns = temporal_columns(truth["rows"])
                columns[case["column"]][0] = parsers[case["column"]].fromisoformat(
                    case["value"]
                )
                with self.assertRaises(yamaa.Error) as caught:
                    yamaa.spec(path).run(inputs={"SRC": pa.table(columns)})
                self.assert_issue(caught.exception.issues.to_pylist(), case["issue"])
                self.assertEqual(caught.exception.paths, [])

    def test_empty_frames_keep_their_declared_schema(self):
        empty = pa.table(
            {name: pa.array([], type=pa.int64()) for name in TRUTH["columns"]}
        )
        self.assert_saved(
            self.spec.run(inputs={"SRC": empty, "AUX": empty}), TRUTH["empty_csv"]
        )

    def test_zero_column_frames_keep_their_rows_for_eager_and_deferred_runs(self):
        truth = TRUTH["zero_columns"]
        path = self.root / "zero-columns.yaml"
        path.write_text(truth["specification"], encoding="utf-8")
        for case in truth["cases"]:
            empty = pa.table({"discard": range(case["rows"])}).drop(["discard"])
            for stream in (False, True):
                with self.subTest(rows=case["rows"], stream=stream):
                    run = yamaa.spec(path).run(stream=stream, inputs={"SRC": empty})
                    self.assert_saved(run, case["csv"])

    def test_invalid_host_shapes_raise_the_native_refusal_class(self):
        for value in ([], {1: pa.table({"ID": [1]})}, {"SRC": object()}):
            with self.subTest(value=type(value).__name__):
                with self.assertRaises(yamaa.Error):
                    self.spec.run(inputs=value)


if __name__ == "__main__":
    unittest.main()
