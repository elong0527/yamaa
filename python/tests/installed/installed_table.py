"""Installed yamaa.Table: the native table of every Python result (#3030 D11,
D43).

Every table the package returns is one. Each member: the counts and names, the
printed preview the facade renders (D167), the Arrow PyCapsule stream,
`to_pylist()`, which builds Python values in Rust and imports nothing, and
`to_arrow()`, `to_polars()` and `to_pandas()`, which import their library only
when called. Each column type reaches Python as the interface's
`public.value_types` project it, and the package needs no table library.
"""

import datetime
import gc
import importlib.abc
import math
import pickle
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path
from unittest.mock import patch

import pandas as pd
import polars as pl
import pyarrow as pa
from host_fixtures import first_difference

import yamaa
from yamaa import _native

# A section lists only specifications of its model (SUB-3).
SPEC = """yamaa: "1.0"
type: spec
model: ADaM
domain: TEST
base: SRC
keys: [ID]
input:
  SRC:
    path: input.csv
    types: {ID: int, S: str, I: int, F: float, D: date, T: datetime}
columns:
  - {name: ID, type: int, label: Identifier, derivation: SRC.ID}
  - {name: S, type: str, label: Text, derivation: SRC.S}
  - {name: I, type: int, label: Integer, derivation: SRC.I}
  - {name: F, type: float, label: Float, derivation: SRC.F}
  - {name: D, type: date, label: Date, derivation: SRC.D}
  - {name: T, type: datetime, label: Datetime, derivation: SRC.T}
output: {path: output.csv, columns: [ID, S, I, F, D, T]}
"""
INPUT = (
    "ID,S,I,F,D,T\n"
    "1,a\0\U0001f980,-9223372036854775808,-0.0,0001-01-01,0001-01-01T00:00:00\n"
    "2,,,,,\n"
    "3,\u03b2,9223372036854775807,1.25,9999-12-31,9999-12-31T23:59:59\n"
)
COLUMNS = ["ID", "S", "I", "F", "D", "T"]
ROWS = [
    {
        "ID": 1,
        "S": "a\0\U0001f980",
        "I": -(2**63),
        "F": -0.0,
        "D": datetime.date(1, 1, 1),
        "T": datetime.datetime(1, 1, 1, 0, 0, 0),
    },
    {"ID": 2, "S": None, "I": None, "F": None, "D": None, "T": None},
    {
        "ID": 3,
        "S": "\u03b2",
        "I": 2**63 - 1,
        "F": 1.25,
        "D": datetime.date(9999, 12, 31),
        "T": datetime.datetime(9999, 12, 31, 23, 59, 59),
    },
]
# public.value_types: str, int, float, date and datetime in Arrow.
ARROW_TYPES = [
    pa.int64(),
    pa.string(),
    pa.int64(),
    pa.float64(),
    pa.date32(),
    pa.timestamp("s"),
]
# D167's preview of ROWS: names, types, then each row; every cell in ASCII
# with the YAML reader's escapes, a float in VALUE-8's text.
PREVIEW = "\n".join(
    [
        "yamaa.Table: 3 rows x 6 columns",
        "ID   S                I                     F      D           T",
        "int  str              int                   float  date        datetime",
        "1    a\\x00\\U0001f980  -9223372036854775808  -0     0001-01-01  "
        "0001-01-01T00:00:00",
        "2    null             null                  null   null        null",
        "3    \\u03b2           9223372036854775807   1.25   9999-12-31  "
        "9999-12-31T23:59:59",
    ]
)
ISSUE_COLUMNS = [
    "severity",
    "phase",
    "condition",
    "rule",
    "spec_paths",
    "file",
    "line",
    "column",
    "message",
    "context",
    "source_span",
    "operand_route",
]
# The issues table's types: text, list<str>, optional int and list<int>.
ISSUE_TYPES = [pa.string()] * 4 + [
    pa.list_(pa.string()),
    pa.string(),
    pa.int64(),
    pa.int64(),
    pa.string(),
    pa.string(),
    pa.list_(pa.int64()),
    pa.list_(pa.string()),
]
# A division by zero at one node of an expression: the issue's span covers
# that node and its route leads to it, empty when the node is the root.
DIVISION = """yamaa: "1.0"
type: spec
domain: TEST
keys: [ID]
input: {SRC: {path: input.csv, types: {ID: str, VALUE: float, ZERO: float}}}
columns:
  - {name: ID, type: str, derivation: SRC.ID}
  - {name: VALUE, type: float, derivation: SRC.VALUE}
  - {name: ZERO, type: float, derivation: SRC.ZERO}
  - {name: RATIO, type: float, derivation: 'EXPRESSION'}
output: {path: output.csv, columns: [ID, VALUE, ZERO, RATIO]}
"""
LIBRARIES = ("polars", "pyarrow", "pandas")


def odm(items):
    """An ODM 1.3 snapshot of one subject whose one item group holds `items`
    items, I1 to I<items>, each with its number as its value."""
    data = "".join(
        f'<ItemData ItemOID="I{number}" Value="{number}"/>'
        for number in range(1, items + 1)
    )
    return (
        '<?xml version="1.0" encoding="UTF-8"?>'
        '<ODM xmlns="http://www.cdisc.org/ns/odm/v1.3" ODMVersion="1.3" '
        'FileOID="F"><ClinicalData StudyOID="S" MetaDataVersionOID="M">'
        '<SubjectData SubjectKey="01"><StudyEventData StudyEventOID="E">'
        '<FormData FormOID="FORM"><ItemGroupData ItemGroupOID="G">'
        f"{data}</ItemGroupData></FormData></StudyEventData></SubjectData>"
        "</ClinicalData></ODM>"
    )


class Table(unittest.TestCase):
    def setUp(self):
        work = tempfile.TemporaryDirectory(prefix="yamaa-table-")
        self.addCleanup(work.cleanup)
        self.root = Path(work.name)
        (self.root / "spec.yaml").write_text(SPEC, encoding="utf-8")
        (self.root / "input.csv").write_text(INPUT, encoding="utf-8")

    def run_spec(self):
        run = yamaa.spec(self.root / "spec.yaml").run()
        self.assertEqual(run.issues.num_rows, 0)
        return run

    def output(self):
        """The output of ROWS."""
        return self.run_spec().output

    def division(self, expression):
        """The issues table of a run whose RATIO divides by zero."""
        path = self.root / "division.yaml"
        path.write_text(DIVISION.replace("EXPRESSION", expression), encoding="utf-8")
        (self.root / "input.csv").write_bytes(b"ID,VALUE,ZERO\na,2.5,0\n")
        run = yamaa.spec(path).run()
        self.assertIsNone(run.output)
        return run.issues

    def test_pickle_preserves_values_schema_lists_and_empty_tables(self):
        # Pickle is a Python transport seam over tables the existing cases own.
        run = self.run_spec()
        for label, table in (
            ("output", run.output),
            ("empty issues", run.issues),
            ("list columns", yamaa.conditions()),
        ):
            for protocol in range(pickle.HIGHEST_PROTOCOL + 1):
                with self.subTest(case=label, protocol=protocol):
                    restored = pickle.loads(pickle.dumps(table, protocol=protocol))
                    self.assertEqual(
                        (
                            type(restored),
                            restored.to_pylist(),
                            repr(restored),
                            restored.to_arrow().schema,
                            restored.to_arrow().to_batches(),
                        ),
                        (
                            yamaa.Table,
                            table.to_pylist(),
                            repr(table),
                            table.to_arrow().schema,
                            table.to_arrow().to_batches(),
                        ),
                    )

    def test_the_native_class_is_exported_as_it_is_and_cannot_be_made(self):
        self.assertIs(yamaa.Table, _native.Table)
        self.assertIn("Table", yamaa.__all__)
        self.assertEqual(yamaa.Table.__module__, "yamaa")
        self.assertEqual(yamaa.Table.__qualname__, "Table")
        with self.assertRaises(TypeError):
            yamaa.Table()
        table = self.output()
        self.assertIsInstance(table, yamaa.Table)
        for name in ("num_rows", "column_names"):
            with self.subTest(name=name), self.assertRaises(AttributeError):
                setattr(table, name, 0)
        public = sorted(name for name in dir(table) if not name.startswith("_"))
        self.assertEqual(
            public,
            [
                "column_names",
                "num_rows",
                "to_arrow",
                "to_pandas",
                "to_polars",
                "to_pylist",
            ],
        )

    def test_every_table_the_package_returns_is_a_table(self):
        run = self.run_spec()
        check = yamaa.spec(self.root / "spec.yaml").check()
        (self.root / "project.yaml").write_text(
            'yamaa: "1.0"\ntype: project\nadam: {specs: [spec.yaml]}\n',
            encoding="utf-8",
        )
        project = yamaa.project(self.root / "project.yaml")
        runs = project.run()
        submission = project.submission("adam", created="2026-10-21T00:00:00")
        (self.root / "items.xml").write_text(odm(1), encoding="utf-8")
        with self.assertRaises(yamaa.Error) as refused:
            project.spec("ABSENT")
        tables = {
            "Check.issues": check.issues,
            "Run.output": run.output,
            "Run.issues": run.issues,
            "Run.report": run.report,
            "MemberRun.output": runs["TEST"].output,
            "MemberRun.issues": runs["TEST"].issues,
            "MemberRun.report": runs["TEST"].report,
            "ProjectRun.issues": runs.issues,
            "ProjectRun.report": runs.report,
            "Submission.issues": submission.issues,
            "SelfTest.cases": yamaa.selftest().cases,
            "read_odm()": yamaa.read_odm(self.root / "items.xml"),
            "Lint.findings": yamaa.lint(self.root / "spec.yaml").findings,
            "Error.issues": refused.exception.issues,
        }
        for name, table in tables.items():
            with self.subTest(table=name):
                self.assertIs(type(table), yamaa.Table)
        self.assertEqual(tables["MemberRun.output"].to_pylist(), ROWS)
        self.assertEqual(tables["Error.issues"].num_rows, 1)

    def test_counts_and_names(self):
        table = self.output()
        self.assertEqual(table.num_rows, 3)
        self.assertEqual(len(table), 3)
        self.assertEqual(table.column_names, COLUMNS)
        self.assertTrue(table)
        # A clean run's issues: no rows, every column, and false as len() is 0.
        issues = self.run_spec().issues
        self.assertEqual((issues.num_rows, len(issues)), (0, 0))
        self.assertEqual(issues.column_names, ISSUE_COLUMNS)
        self.assertFalse(issues)
        self.assertEqual(issues.to_pylist(), [])

    def test_repr_and_str_print_the_facade_preview(self):
        table = self.output()
        self.assertEqual(repr(table), PREVIEW)
        self.assertEqual(str(table), PREVIEW)
        self.assertTrue(repr(table).isascii())
        # Past 8 columns the name and type rows end in a `...` column.
        self.assertEqual(
            repr(self.run_spec().issues),
            "\n".join(
                [
                    "yamaa.Table: 0 rows x 12 columns",
                    "severity  phase  condition  rule  spec_paths  file  line  column"
                    "  ...",
                    "str       str    str        str   list<str>   str   int   int   "
                    "  ...",
                ]
            ),
        )

    def test_the_arrow_stream_gives_each_column_its_value_type(self):
        table = self.output()
        self.assertEqual(pa.table(table).schema.types, ARROW_TYPES)
        self.assertEqual(pa.table(table).column_names, COLUMNS)
        self.assertEqual(
            pa.table(self.division("VALUE / ZERO")).schema.types, ISSUE_TYPES
        )
        # Each call exports a fresh capsule, and a requested schema is refused.
        first = table.__arrow_c_stream__()
        self.assertIsNot(first, table.__arrow_c_stream__(None))
        del first
        gc.collect()
        with self.assertRaises(yamaa.Error):
            table.__arrow_c_stream__(
                pa.schema([("ID", pa.int32())]).__arrow_c_schema__()
            )

    def test_to_pylist_builds_each_value_type_in_python(self):
        rows = self.output().to_pylist()
        self.assertEqual(rows, ROWS)
        self.assertEqual([list(row) for row in rows], [COLUMNS] * 3)
        first, missing, last = rows
        for row in (first, last):
            self.assertIs(type(row["ID"]), int)
            self.assertIs(type(row["S"]), str)
            self.assertIs(type(row["I"]), int)
            self.assertIs(type(row["F"]), float)
            # A date is a date, never a datetime, and a datetime is naive.
            self.assertIs(type(row["D"]), datetime.date)
            self.assertIs(type(row["T"]), datetime.datetime)
            self.assertIsNone(row["T"].tzinfo)
        self.assertEqual(math.copysign(1.0, first["F"]), -1.0)
        self.assertTrue(all(missing[name] is None for name in COLUMNS[1:]))
        # Lists hold their items' values; a missing list is None and an
        # empty one an empty list.
        for expression, span, route in (
            ("1 + VALUE / ZERO", [4, 16], ["right"]),
            ("VALUE / ZERO", [0, 12], []),
        ):
            with self.subTest(expression=expression):
                (row,) = self.division(expression).to_pylist()
                self.assertEqual(list(row), ISSUE_COLUMNS)
                self.assertEqual(row["condition"], "division_by_zero")
                self.assertEqual(row["spec_paths"], ["columns.RATIO.derivation"])
                self.assertEqual(row["source_span"], span)
                self.assertEqual(row["operand_route"], route)
                self.assertIs(type(row["operand_route"]), list)
                self.assertTrue(all(type(item) is int for item in row["source_span"]))
                # The issue is located where the specification wrote RATIO.
                self.assertEqual(
                    row["file"], (self.root / "division.yaml").resolve().as_posix()
                )
                self.assertIs(type(row["line"]), int)
                self.assertIs(type(row["column"]), int)

    def test_to_pylist_reads_every_batch_in_order(self):
        # The ODM reader returns its items in batches of 65,536 rows.
        path = self.root / "items.xml"
        path.write_text(odm(65_537), encoding="utf-8")
        table = pickle.loads(pickle.dumps(yamaa.read_odm(path)))
        self.assertEqual((table.num_rows, len(table)), (65_537, 65_537))
        self.assertEqual(len(pa.table(table).to_batches()), 2)
        rows = table.to_pylist()
        # The 65,537 rows are compared as a summary of their first
        # difference (#3882).
        self.assertIsNone(first_difference(rows, pa.table(table).to_pylist()))
        self.assertEqual(
            [(row["ItemOID"], row["SourceOrdinal"]) for row in rows[65_535:]],
            [("I65536", 65_536), ("I65537", 65_537)],
        )

    def test_to_arrow_polars_and_pandas_keep_every_value(self):
        table = self.output()
        arrow = table.to_arrow()
        self.assertIsInstance(arrow, pa.Table)
        self.assertEqual(arrow.schema.types, ARROW_TYPES)
        self.assertEqual(arrow.to_pylist(), ROWS)

        frame = table.to_polars()
        self.assertIsInstance(frame, pl.DataFrame)
        self.assertEqual(frame.columns, COLUMNS)
        self.assertEqual(
            frame.dtypes[:5], [pl.Int64, pl.String, pl.Int64, pl.Float64, pl.Date]
        )
        self.assertIsInstance(frame.dtypes[5], pl.Datetime)
        self.assertEqual(frame.to_dicts(), ROWS)

        # pandas keeps each Arrow type as a pandas.ArrowDtype, so no integer
        # beside a missing value becomes a float.
        frame = table.to_pandas()
        self.assertIsInstance(frame, pd.DataFrame)
        self.assertEqual(list(frame.columns), COLUMNS)
        self.assertEqual(
            list(frame.dtypes), [pd.ArrowDtype(kind) for kind in ARROW_TYPES]
        )
        self.assertEqual(frame["I"].tolist()[0], -(2**63))
        self.assertIs(frame["I"].tolist()[1], pd.NA)
        self.assertEqual(
            pa.Table.from_pandas(frame, preserve_index=False).to_pylist(), ROWS
        )
        issues = self.division("1 + VALUE / ZERO")
        self.assertEqual(
            pa.Table.from_pandas(issues.to_pandas(), preserve_index=False).to_pylist(),
            issues.to_pylist(),
        )
        self.assertEqual(issues.to_polars().to_dicts(), issues.to_pylist())
        self.assertEqual(issues.to_arrow().to_pylist(), issues.to_pylist())

    def test_a_missing_library_raises_an_import_error_naming_it(self):
        table = self.output()
        conversions = {
            "to_arrow": table.to_arrow,
            "to_polars": table.to_polars,
            "to_pandas": table.to_pandas,
        }
        for method, blocked, package in (
            ("to_polars", {"polars"}, "polars"),
            ("to_arrow", {"pyarrow"}, "pyarrow"),
            ("to_pandas", {"pandas"}, "pandas"),
            ("to_pandas", {"pyarrow"}, "pyarrow"),
            ("to_pandas", {"pandas", "pyarrow"}, "pandas"),
        ):
            with self.subTest(method=method, blocked=sorted(blocked)):
                # None in sys.modules is how Python reports a module that
                # cannot be imported.
                with patch.dict(sys.modules, dict.fromkeys(blocked)):
                    with self.assertRaises(ImportError) as caught:
                        conversions[method]()
                error = caught.exception
                self.assertIs(type(error), ModuleNotFoundError)
                self.assertEqual(error.name, package)
                self.assertEqual(
                    str(error), f"{method}() needs {package}, which is not installed"
                )
                self.assertIsInstance(error.__cause__, ModuleNotFoundError)

    def test_an_installed_library_that_fails_to_import_raises_its_own_error(self):
        table = self.output()

        class Broken(importlib.abc.MetaPathFinder):
            def find_spec(self, fullname, path=None, target=None):
                if fullname == "polars":
                    raise ImportError("polars is broken", name="polars")
                return None

        broken = Broken()
        with patch.dict(sys.modules):
            del sys.modules["polars"]
            sys.meta_path.insert(0, broken)
            try:
                with self.assertRaises(ImportError) as caught:
                    table.to_polars()
            finally:
                sys.meta_path.remove(broken)
        self.assertIs(type(caught.exception), ImportError)
        self.assertEqual(str(caught.exception), "polars is broken")

    def test_import_and_to_pylist_need_no_table_library(self):
        # The public module and every member but the three conversions run
        # with polars, PyArrow and pandas unavailable.
        script = textwrap.dedent(
            """
            import importlib.abc
            import pickle
            import sys
            from pathlib import Path

            blocked = {"polars", "pyarrow", "pandas"}

            class Unavailable(importlib.abc.MetaPathFinder):
                def find_spec(self, fullname, path=None, target=None):
                    if fullname.split(".")[0] in blocked:
                        raise ModuleNotFoundError(fullname, name=fullname)
                    return None

            sys.meta_path.insert(0, Unavailable())
            import yamaa

            root = Path(sys.argv[1])
            table = yamaa.spec(root / "spec.yaml").run().output
            table = pickle.loads(pickle.dumps(table))
            rows = table.to_pylist()
            assert rows[2]["I"] == 2**63 - 1, rows
            assert (table.num_rows, len(table), table.column_names[0]) == (3, 3, "ID")
            assert repr(table).startswith("yamaa.Table: 3 rows x 6 columns")
            del rows
            table.__arrow_c_stream__()
            for convert, package in (
                (table.to_arrow, "pyarrow"),
                (table.to_polars, "polars"),
                (table.to_pandas, "pandas"),
            ):
                try:
                    convert()
                except ModuleNotFoundError as error:
                    assert error.name == package, error
                else:
                    raise AssertionError(package)
            assert not {name.split(".")[0] for name in sys.modules} & blocked
            """
        )
        result = subprocess.run(
            [sys.executable, "-I", "-c", script, str(self.root)],
            capture_output=True,
            text=True,
            timeout=120,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
