"""The expression evaluation set's check: the rules a case follows, the
coverage the set must give, how a run is judged and the specification a case
runs as. No test here runs the engine; `make check-repository` does."""

import copy
import datetime as dt
import json
import tempfile
import unittest
from pathlib import Path

from tools.commands import repository
from tools.paths import ROOT
from tools.validation import check_expression_conformance as OWNER

DOCUMENT = OWNER.DOCUMENT


def committed():
    document, errors = OWNER.load(ROOT)
    if errors:
        raise AssertionError(errors)
    return document


def case(**fields):
    """A valid derivation case, with `fields` replacing or adding keys."""
    probe = {
        "id": "probe",
        "context": "derivation",
        "covers": ["ABS"],
        "text": "ABS(X)",
        "columns": {"X": "int"},
        "rows": [{"X": -1}, {"X": None}],
        "type": "int",
        "values": [1, None],
    }
    probe.update(fields)
    return {key: value for key, value in probe.items() if value is not ...}


class DocumentRulesTests(unittest.TestCase):
    def errors(self, *cases, drop=()):
        document = copy.deepcopy(committed())
        document["cases"] = [
            item for item in document["cases"] if item["id"] not in drop
        ] + list(cases)
        return OWNER.check(document, ROOT)

    def test_the_committed_set_follows_every_rule(self):
        self.assertEqual(self.errors(), [])

    def test_a_case_that_breaks_one_rule_is_named_with_it(self):
        label = f"{DOCUMENT}: probe"
        # A valid aggregate case, which may give the fields a reducer reads.
        aggregate = {
            "context": "aggregate",
            "covers": ["COUNT"],
            "text": "COUNT(R.X)",
            "rows": ...,
            "groups": [[{"X": 1}]],
            "values": [1],
        }
        for name, probe, expected in (
            ("a valid case", case(), []),
            ("an unknown key", case(note="x"), [f"{label}: unknown keys ['note']"]),
            (
                "an unknown cover",
                case(covers=["SQUARE"]),
                [f"{label}: unknown covers ['SQUARE']"],
            ),
            (
                "a cover its text does not write, beside a topic",
                case(covers=["ABS", "CEIL", "missing"]),
                [f"{label}: covers ['CEIL'], which its text does not write"],
            ),
            (
                "an id that is not lowercase words",
                case(id="Probe"),
                [f"{DOCUMENT}: Probe: an id is unique lowercase words joined by -"],
            ),
            (
                "an unknown context",
                case(context="compute"),
                [
                    f"{label}: context must be one of "
                    "('predicate', 'derivation', 'aggregate')"
                ],
            ),
            (
                "a column the check reserves",
                case(columns={"RESULT": "int"}, rows=[{"RESULT": 1}], values=[1]),
                [f"{label}: column RESULT is a name the check reserves"],
            ),
            (
                "a row that names another column",
                case(rows=[{"X": 1}, {"Z": 1}]),
                [f"{label}: each record names exactly ['X']"],
            ),
            (
                "empty input text",
                case(columns={"S": "str"}, rows=[{"S": ""}, {"S": None}]),
                [f"{label}: S holds empty text; write null"],
            ),
            (
                "a value of another type",
                case(type="float", values=[1, None]),
                [f"{label}: 1 is not a float value"],
            ),
            (
                "one value too few",
                case(values=[1]),
                [f"{label}: values must give one value per entry"],
            ),
            (
                "a derivation without its type",
                case(type=...),
                [
                    f"{label}: type must be one of "
                    "('str', 'int', 'float', 'date', 'datetime')"
                ],
            ),
            (
                "a predicate value that is no truth value",
                case(context="predicate", type=..., values=["TRUE", "YES"]),
                [f"{label}: 'YES' is not a truth value"],
            ),
            (
                "an aggregate written with rows",
                case(context="aggregate", covers=["SUM"], text="SUM(R.X)"),
                [
                    f"{label}: a case read in aggregate gives groups, not rows",
                    f"{label}: groups must be a non-empty list",
                ],
            ),
            (
                "grouping columns outside an aggregate",
                case(group_by=["X"]),
                [f"{label}: only an aggregate case gives group_by"],
            ),
            (
                "a grouping column the case does not declare",
                case(
                    context="aggregate",
                    covers=["SUM"],
                    text="SUM(R.X)",
                    rows=...,
                    groups=[[{"X": 1}]],
                    values=[1],
                    group_by=["G"],
                ),
                [f"{label}: group_by must list distinct columns of the case"],
            ),
            (
                "a group whose records differ in a grouping column, and an empty one",
                case(
                    context="aggregate",
                    covers=["SUM", "*"],
                    text="SUM(R.X) * R.N",
                    columns={"N": "int", "X": "int"},
                    rows=...,
                    groups=[[{"N": 1, "X": 1}, {"N": 2, "X": 1}], []],
                    values=[1, None],
                    group_by=["N"],
                ),
                [
                    f"{label}: the records of group 1 must share one value of "
                    "each group_by column",
                    f"{label}: the records of group 2 must share one value of "
                    "each group_by column",
                ],
            ),
            (
                "a reducer field outside an aggregate",
                case(separator=", "),
                [f"{label}: only an aggregate case gives ['separator']"],
            ),
            (
                "an order term that names no column of the case",
                case(**aggregate, order_by=["R.Z", "X"]),
                [
                    f"{label}: order term 'R.Z' names no column as R.<column>",
                    f"{label}: order term 'X' names no column as R.<column>",
                ],
            ),
            (
                "an order term with another direction",
                case(**aggregate, order_by=[{"variable": "R.X", "direction": "up"}]),
                [
                    f"{label}: order term {{'variable': 'R.X', 'direction': "
                    "'up'} is not an order term"
                ],
            ),
            (
                "a separator that is not text and a distinct that is not Boolean",
                case(**aggregate, separator=1, distinct="yes"),
                [
                    f"{label}: separator must be text",
                    f"{label}: distinct must be true or false",
                ],
            ),
            (
                "a quantile outside an aggregate",
                case(quantile=0.5),
                [f"{label}: only an aggregate case gives ['quantile']"],
            ),
            (
                "a quantile that is text",
                case(**aggregate, quantile="half"),
                [f"{label}: quantile must be a number"],
            ),
            (
                "a quantile that is Boolean",
                case(**aggregate, quantile=True),
                [f"{label}: quantile must be a number"],
            ),
            (
                "a quantile outside 0 through 1, which a refused case shows",
                case(**aggregate, quantile=1.5),
                [],
            ),
            (
                "values and an issue",
                case(phase="derivation"),
                [f"{label}: a case gives values or an issue, not both"],
            ),
            (
                "an unregistered condition",
                case(
                    values=...,
                    phase="derivation",
                    condition="overflowed",
                    rule="VALUE-5",
                ),
                [f"{label}: 'overflowed' is not a registered condition"],
            ),
            (
                "a phase its condition is not raised in",
                case(
                    values=...,
                    phase="validation",
                    condition="division_by_zero",
                    rule="FORMULA-15",
                ),
                [f"{label}: division_by_zero is raised only in ['derivation']"],
            ),
            (
                "a rule no card anchors",
                case(
                    values=...,
                    phase="derivation",
                    condition="division_by_zero",
                    rule="NONE-1",
                ),
                [f"{label}: no rule card anchors NONE-1"],
            ),
        ):
            with self.subTest(case=name):
                self.assertEqual(self.errors(probe), expected)

    def test_a_text_writes_what_the_grammar_reads_its_tokens_as(self):
        # Each expectation follows yaml/grammar/expression.yaml: the optional
        # NOT of membership, range and pattern, and of null_test after IS; the
        # AND of range; factor's sign, which a number literal keeps; star; and
        # DATE, DATETIME and ESCAPE, which `reserved` leaves out.
        for text, expected in (
            ("X NOT IN (1, NULL)", {"NOT IN", "NULL"}),
            ("NOT X IN (1)", {"NOT", "IN"}),
            (
                "S NOT LIKE 'a%' AND NOT X IS NOT NULL",
                {"NOT LIKE", "AND", "NOT", "IS NOT NULL"},
            ),
            ("NULL IS NULL", {"NULL", "IS NULL"}),
            ("X BETWEEN 1 AND 3", {"BETWEEN"}),
            ("X NOT BETWEEN 1 AND 3 and Y = 2", {"NOT BETWEEN", "AND", "="}),
            ("-1 - X * -Y", {"-", "*", "unary -"}),
            ("+X <> (X) + 1", {"unary +", "<>", "+"}),
            ("count(R.*) / Sum(R.X)", {"COUNT", "/", "SUM"}),
            ("S LIKE 'A AND ABS(B)' ESCAPE '!'", {"LIKE", "ESCAPE"}),
            ("S = 'it''s' OR S >= 'NOT'", {"=", "OR", ">="}),
            ("DATE '2025-06-01' <= DATE", {"DATE", "<="}),
            ("DATETIME < 1 OR TRUE", {"<", "OR", "TRUE"}),
        ):
            with self.subTest(text=text):
                self.assertEqual(OWNER.written(text), expected)

    def test_the_set_must_cover_the_grammar_and_the_refusals(self):
        cases = committed()["cases"]

        def ids(test):
            return [item["id"] for item in cases if test(item)]

        for name, drop, expected in (
            (
                "no value case of a function",
                ids(lambda item: "MOD" in item["covers"] and "values" in item),
                [f"{DOCUMENT}: no value case covers the functions ['MOD']"],
            ),
            (
                "no other argument count of a function",
                ids(lambda item: item["id"] == "exp-takes-one-argument"),
                [f"{DOCUMENT}: no case refuses another argument count of ['EXP']"],
            ),
            (
                "no unknown truth of a logical operator",
                ids(
                    lambda item: (
                        "NOT" in item["covers"] and "UNKNOWN" in item.get("values", [])
                    )
                ),
                # Only not-false-holds-for-every-row, all TRUE, is left.
                [
                    f"{DOCUMENT}: the predicate cases of NOT give no "
                    "['FALSE', 'UNKNOWN']"
                ],
            ),
            (
                "no refused construct in a context",
                ids(
                    lambda item: (
                        item["context"] == "aggregate"
                        and item.get("condition") == "prohibited_construct"
                    )
                ),
                [
                    f"{DOCUMENT}: no aggregate case is refused with "
                    "prohibited_construct under FORMULA-27"
                ],
            ),
        ):
            with self.subTest(case=name):
                self.assertEqual(self.errors(drop=drop), expected)

    def test_the_repository_checks_run_it(self):
        self.assertIn(
            "tools.validation.check_expression_conformance", repository.CHECKS
        )


class ReadingTests(unittest.TestCase):
    def test_a_document_two_yaml_versions_read_apart_or_non_ascii_is_refused(self):
        text = (ROOT / DOCUMENT).read_text(encoding="utf-8")
        old = "values: [0.75, 1.5]"
        for name, replacement, expected in (
            (
                "an unsigned exponent",
                "values: [0.75, 1.5e0]",
                "YAML 1.1 reads a value differently from YAML 1.2",
            ),
            ("a literal non-ASCII scalar", "values: [0.75, 1.5] # \u00e9", "escape"),
        ):
            with self.subTest(case=name), tempfile.TemporaryDirectory() as scratch:
                root = Path(scratch)
                (root / DOCUMENT).parent.mkdir(parents=True)
                self.assertEqual(text.count(old), 1)
                (root / DOCUMENT).write_text(
                    text.replace(old, replacement), encoding="utf-8"
                )
                document, errors = OWNER.load(root)
                self.assertEqual(
                    (document, len(errors), expected in errors[0]), (None, 1, True)
                )


class JudgmentTests(unittest.TestCase):
    def test_a_run_matches_only_the_exact_values_or_the_one_issue(self):
        flag = case(context="predicate", type=..., values=["TRUE", "UNKNOWN"])
        issue = {
            "phase": "derivation",
            "condition": "division_by_zero",
            "rule": "FORMULA-15",
        }
        refusal = case(values=..., **issue)
        rows = [{"ROWID": 1, "RESULT": 1}, {"ROWID": 2, "RESULT": None}]
        for name, probe, status, output, issues, expected in (
            ("equal values", case(), "succeeded", rows, [], []),
            (
                "another value",
                case(),
                "succeeded",
                [{"ROWID": 1, "RESULT": 2}, {"ROWID": 2, "RESULT": None}],
                [],
                ["entry 1: expected 1, got 2"],
            ),
            (
                "a float where an int is expected",
                case(),
                "succeeded",
                [{"ROWID": 1, "RESULT": 1.0}, {"ROWID": 2, "RESULT": None}],
                [],
                ["entry 1: expected 1, got 1.0"],
            ),
            (
                "negative zero where zero is expected",
                case(type="float", values=[0.0, None]),
                "succeeded",
                [{"ROWID": 1, "RESULT": -0.0}, {"ROWID": 2, "RESULT": None}],
                [],
                ["entry 1: expected 0.0, got -0.0"],
            ),
            (
                "a date read back",
                case(type="date", values=["2025-06-01", None]),
                "succeeded",
                [
                    {"ROWID": 1, "RESULT": dt.date(2025, 6, 1)},
                    {"ROWID": 2, "RESULT": None},
                ],
                [],
                [],
            ),
            (
                "truth values",
                flag,
                "succeeded",
                [{"ROWID": 1, "RESULT": "TRUE"}, {"ROWID": 2, "RESULT": "UNKNOWN"}],
                [],
                [],
            ),
            (
                "a missing row",
                case(),
                "succeeded",
                rows[:1],
                [],
                ["expected entries [1, 2], got [1]"],
            ),
            (
                "an issue where values are expected",
                case(),
                "failed",
                [],
                [issue],
                [f"expected values, got failed with {[issue]}"],
            ),
            ("the one issue", refusal, "failed", [], [issue], []),
            (
                "a second issue",
                refusal,
                "failed",
                [],
                [issue, issue],
                [f"expected the one issue {issue}, got failed with {[issue, issue]}"],
            ),
            (
                "values where an issue is expected",
                refusal,
                "succeeded",
                rows,
                [],
                [f"expected the one issue {issue}, got succeeded with []"],
            ),
        ):
            with self.subTest(case=name):
                self.assertEqual(OWNER.judge(probe, status, output, issues), expected)


class StagingTests(unittest.TestCase):
    def staged(self, probe):
        with tempfile.TemporaryDirectory() as scratch:
            directory = Path(scratch)
            path = OWNER.stage(probe, directory)
            files = {
                item.name: item.read_text(encoding="utf-8")
                for item in sorted(directory.iterdir())
                if item.suffix == ".csv"
            }
            return json.loads(path.read_text(encoding="utf-8")), files

    def test_each_context_runs_as_one_column_of_a_specification(self):
        header = {"yamaa": "1.0", "type": "spec", "domain": "CONFORMANCE"}
        rows = (
            {
                "keys": ["ROWID"],
                "input": {
                    "SRC": {"path": "rows.csv", "types": {"ROWID": "int", "X": "int"}}
                },
                "base": "SRC",
            },
            {"rows.csv": "ROWID,X\n1,-1\n2,\n"},
        )
        bound = [
            {"name": "ROWID", "type": "int", "derivation": "SRC.ROWID"},
            {"name": "X", "type": "int", "derivation": "SRC.X"},
        ]
        output = {"path": "unwritten.csv", "columns": ["ROWID", "RESULT"]}
        for name, probe, expected in (
            (
                "derivation text is a column's derivation",
                case(),
                (
                    {
                        **header,
                        **rows[0],
                        "columns": [
                            *bound,
                            {"name": "RESULT", "type": "int", "derivation": "ABS(X)"},
                        ],
                        "output": output,
                    },
                    rows[1],
                ),
            ),
            (
                "a predicate is a flag's condition naming the truth values",
                case(
                    context="predicate",
                    text="X > 0",
                    type=...,
                    values=["FALSE", "UNKNOWN"],
                ),
                (
                    {
                        **header,
                        **rows[0],
                        "columns": [
                            *bound,
                            {
                                "name": "RESULT",
                                "type": "str",
                                "derivation": {
                                    "flag": {
                                        "condition": "X > 0",
                                        "true_value": "TRUE",
                                        "false_value": "FALSE",
                                        "missing": "UNKNOWN",
                                    }
                                },
                            },
                        ],
                        "output": output,
                    },
                    rows[1],
                ),
            ),
            (
                "aggregate text reduces the records of R matched to each group",
                case(
                    context="aggregate",
                    text="SUM(R.F)",
                    columns={"F": "float"},
                    rows=...,
                    groups=[[{"F": 1e16}, {"F": None}], []],
                    type="float",
                    values=[1e16, None],
                ),
                (
                    {
                        **header,
                        "keys": ["GROUPID"],
                        "input": {
                            "GROUPS": {
                                "path": "groups.csv",
                                "types": {"GROUPID": "int"},
                            },
                            "R": {
                                "path": "records.csv",
                                "types": {"GROUPID": "int", "F": "float"},
                                "key": ["GROUPID"],
                            },
                        },
                        "base": "GROUPS",
                        "columns": [
                            {
                                "name": "GROUPID",
                                "type": "int",
                                "derivation": "GROUPS.GROUPID",
                            },
                            {
                                "name": "RESULT",
                                "type": "float",
                                "derivation": {"aggregate": "SUM(R.F)"},
                            },
                        ],
                        "output": {
                            "path": "unwritten.csv",
                            "columns": ["GROUPID", "RESULT"],
                        },
                    },
                    {
                        "groups.csv": "GROUPID\n1\n2\n",
                        "records.csv": "GROUPID,F\n1,1e+16\n1,\n",
                    },
                ),
            ),
            (
                "an aggregate case's reducer fields stand beside its text",
                case(
                    context="aggregate",
                    covers=["STR_JOIN"],
                    text="STR_JOIN(R.X)",
                    rows=...,
                    groups=[[{"X": 2}]],
                    type="str",
                    values=["2"],
                    order_by=[{"variable": "R.X", "direction": "desc"}],
                    separator=", ",
                    distinct=True,
                ),
                (
                    {
                        **header,
                        "keys": ["GROUPID"],
                        "input": {
                            "GROUPS": {
                                "path": "groups.csv",
                                "types": {"GROUPID": "int"},
                            },
                            "R": {
                                "path": "records.csv",
                                "types": {"GROUPID": "int", "X": "int"},
                                "key": ["GROUPID"],
                            },
                        },
                        "base": "GROUPS",
                        "columns": [
                            {
                                "name": "GROUPID",
                                "type": "int",
                                "derivation": "GROUPS.GROUPID",
                            },
                            {
                                "name": "RESULT",
                                "type": "str",
                                "derivation": {
                                    "aggregate": {
                                        "expr": "STR_JOIN(R.X)",
                                        "order_by": [
                                            {"variable": "R.X", "direction": "desc"}
                                        ],
                                        "separator": ", ",
                                        "distinct": True,
                                    }
                                },
                            },
                        ],
                        "output": {
                            "path": "unwritten.csv",
                            "columns": ["GROUPID", "RESULT"],
                        },
                    },
                    {"groups.csv": "GROUPID\n1\n", "records.csv": "GROUPID,X\n1,2\n"},
                ),
            ),
            (
                "an aggregate case's quantile stands beside its text",
                case(
                    context="aggregate",
                    covers=["QUANTILE"],
                    text="QUANTILE(R.X)",
                    rows=...,
                    groups=[[{"X": 2}]],
                    type="float",
                    values=[2.0],
                    quantile=0.9,
                ),
                (
                    {
                        **header,
                        "keys": ["GROUPID"],
                        "input": {
                            "GROUPS": {
                                "path": "groups.csv",
                                "types": {"GROUPID": "int"},
                            },
                            "R": {
                                "path": "records.csv",
                                "types": {"GROUPID": "int", "X": "int"},
                                "key": ["GROUPID"],
                            },
                        },
                        "base": "GROUPS",
                        "columns": [
                            {
                                "name": "GROUPID",
                                "type": "int",
                                "derivation": "GROUPS.GROUPID",
                            },
                            {
                                "name": "RESULT",
                                "type": "float",
                                "derivation": {
                                    "aggregate": {
                                        "expr": "QUANTILE(R.X)",
                                        "quantile": 0.9,
                                    }
                                },
                            },
                        ],
                        "output": {
                            "path": "unwritten.csv",
                            "columns": ["GROUPID", "RESULT"],
                        },
                    },
                    {"groups.csv": "GROUPID\n1\n", "records.csv": "GROUPID,X\n1,2\n"},
                ),
            ),
            (
                "aggregate text with grouping columns is a grouped row template",
                case(
                    context="aggregate",
                    text="SUM(R.X) * R.N",
                    columns={"N": "int", "X": "int"},
                    rows=...,
                    groups=[[{"N": 2, "X": 1}, {"N": 2, "X": None}]],
                    values=[2],
                    group_by=["N"],
                ),
                (
                    {
                        **header,
                        "keys": ["GROUPID"],
                        "input": {
                            "R": {
                                "path": "records.csv",
                                "types": {"GROUPID": "int", "N": "int", "X": "int"},
                            }
                        },
                        "columns": [
                            {
                                "name": "GROUPID",
                                "type": "int",
                                "derivation": "R.GROUPID",
                            },
                            {"name": "RESULT", "type": "int"},
                        ],
                        "rows": [
                            {
                                "id": "groups",
                                "dataset": "R",
                                "group_by": ["R.GROUPID", "R.N"],
                                "derivations": {
                                    "RESULT": {"aggregate": "SUM(R.X) * R.N"}
                                },
                            }
                        ],
                        "output": {
                            "path": "unwritten.csv",
                            "columns": ["GROUPID", "RESULT"],
                        },
                    },
                    {"records.csv": "GROUPID,N,X\n1,2,1\n1,2,\n"},
                ),
            ),
        ):
            with self.subTest(case=name):
                self.assertEqual(self.staged(probe), expected)


if __name__ == "__main__":
    unittest.main()
