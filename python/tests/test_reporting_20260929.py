"""Boundary tests for the full-pin reporting of 2026-09-29 (issue #1497).

The benchmark harnesses now compare every pinned field; these pin the edges
the corpus leaves open: a row window reading another window's result
(REQ-0326), a grouped template's scalar read of a non-group driver field
(REQ-0067/REQ-0157), and the row keys a failure in a row template carries.
"""

import os

import pytest
import yaml

from yamaa import YamaaError, derive


def run(tmp_path, spec, inputs):
    d = str(tmp_path)
    os.makedirs(os.path.join(d, "input"), exist_ok=True)
    for name, rows in inputs.items():
        with open(os.path.join(d, "input", name), "w", encoding="utf-8") as f:
            f.write(rows)
    spec_path = os.path.join(d, "spec.yaml")
    with open(spec_path, "w", encoding="utf-8") as f:
        yaml.safe_dump(spec, f)
    return derive(spec_path)


def expect_error(tmp_path, spec, inputs):
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, spec, inputs)
    return ei.value


LB = "USUBJID,LBSEQ,VAL\nS1,1,30\nS1,2,10\nS2,1,20\n"


def rows_spec(derivations, columns, group_by=None):
    template = {"id": "lb", "dataset": "LB", "derivations": derivations}
    if group_by is not None:
        template["group_by"] = group_by
    return {
        "schema_version": "1.0",
        "domain": "ADLB",
        "keys": [c for c in ("USUBJID", "LBSEQ") if c in columns],
        "input": {
            "LB": {"path": "input/lb.csv", "types": {"LBSEQ": "int", "VAL": "int"}}
        },
        "output": {"path": "adlb.csv", "columns": list(columns)},
        "columns": [{"name": n, "type": t} for n, t in columns.items()],
        "rows": [template],
    }


def test_row_window_reads_a_window_result(tmp_path):
    # REQ-0326: each derivation completes for all rows of the template
    # before a dependent starts, so a window may read another window's
    # result through a scalar derivation.
    spec = rows_spec(
        {
            "USUBJID": "LB.USUBJID",
            "LBSEQ": "LB.LBSEQ",
            "RN": {"row_number": {"window": {"order_by": ["LB.VAL"]}}},
            "DBL": {"compute": {"expr": "RN * 2"}},
            "PREV": {
                "row_value": {
                    "source": "DBL",
                    "offset": -1,
                    "window": {"order_by": ["RN"]},
                }
            },
        },
        {"USUBJID": "str", "LBSEQ": "int", "RN": "int", "DBL": "int", "PREV": "int"},
    )
    out = run(tmp_path, spec, {"lb.csv": LB})
    assert out.splitlines() == [
        "USUBJID,LBSEQ,RN,DBL,PREV",
        "S1,1,3,6,4",
        "S1,2,1,2,",
        "S2,1,2,4,2",
    ]


def test_grouped_read_of_a_non_group_field_fails_validation(tmp_path):
    # REQ-0067: outside an aggregate, a grouped template's driver field that
    # is not a group key varies within the group; the read is rejected
    # before any row is built.
    spec = rows_spec(
        {"USUBJID": "LB.USUBJID", "X": "LB.VAL"},
        {"USUBJID": "str", "X": "str"},
        group_by=["LB.USUBJID"],
    )
    e = expect_error(tmp_path, spec, {"lb.csv": LB})
    assert (e.phase, e.condition, e.requirement) == (
        "validation",
        "ungrouped_driver_field",
        "REQ-0067",
    )
    assert e.spec_paths == ["rows[0].derivations.X"]
    assert e.context["identifier"] == "LB.VAL"


def test_grouped_aggregate_reads_non_group_fields(tmp_path):
    # REQ-0067: an aggregate reads the group's records, so its fields need
    # not be group keys.
    spec = rows_spec(
        {"USUBJID": "LB.USUBJID", "N": {"aggregate": {"expr": "SUM(LB.VAL)"}}},
        {"USUBJID": "str", "N": "int"},
        group_by=["LB.USUBJID"],
    )
    out = run(tmp_path, spec, {"lb.csv": LB})
    assert out.splitlines() == ["USUBJID,N", "S1,40", "S2,20"]


def test_row_failure_carries_the_row_keys(tmp_path):
    # A failure evaluating a row names it by its output keys once every key
    # is derived; the operation's own path locates the failure.
    spec = rows_spec(
        {
            "USUBJID": "LB.USUBJID",
            "LBSEQ": "LB.LBSEQ",
            "X": {"compute": {"expr": "100 / (VAL - 10)"}},
            "VAL": "LB.VAL",
        },
        {"USUBJID": "str", "LBSEQ": "int", "VAL": "int", "X": "float"},
    )
    e = expect_error(tmp_path, spec, {"lb.csv": LB})
    assert (e.phase, e.condition) == ("derivation", "division_by_zero")
    assert e.spec_paths == ["rows[0].derivations.X.compute"]
    assert e.context["keys"] == [{"USUBJID": "S1", "LBSEQ": 2}]
