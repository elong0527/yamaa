"""Owned Arrow input overrides from the shared memory-inputs truth (#4086)."""

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

    def test_unsupported_physical_types_have_an_authored_input_diagnostic(self):
        for values in (pa.array([True]), pa.array([42], type=pa.int32())):
            with self.subTest(type=str(values.type)):
                value = pa.table(
                    {"ID": pa.array([1], type=pa.int64()), "VALUE": values}
                )
                with self.assertRaises(yamaa.Error) as caught:
                    self.spec.run(inputs={"SRC": value})
                self.assert_issue(
                    caught.exception.issues.to_pylist(), TRUTH["unsupported_type"]
                )
                self.assertEqual(caught.exception.paths, [])

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

    def test_invalid_host_shapes_raise_the_native_refusal_class(self):
        for value in ([], {1: pa.table({"ID": [1]})}, {"SRC": object()}):
            with self.subTest(value=type(value).__name__):
                with self.assertRaises(yamaa.Error):
                    self.spec.run(inputs=value)


if __name__ == "__main__":
    unittest.main()
