"""Boundary tests for verification declarations as main unified them.

The benchmark corpus pins the headline conditions; these pin the edges it
leaves open: the intermediate `verifications` list and its two `unique`
forms (REQ-1245), optional ids unique within one list (REQ-0374/REQ-0398),
the unnamed dataset `unique` (REQ-0381), and the report of an unnamed
grouped `row_count` (REQ-0402).
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


def pinned(e):
    return (e.phase, e.condition, e.requirement)


# -- intermediate verifications (REQ-1245) -----------------------------------

DM = "USUBJID\nS1\nS2\n"
DS = "USUBJID,DSCAT\nS1,END\nS1,END\nS2,END\n"


def donor_spec(verifications, column="EOT.DSCAT", **decl):
    return {
        "schema_version": "1.0",
        "domain": "ADSL",
        "keys": ["USUBJID"],
        "base": "DM",
        "input": {"DM": "input/dm.csv", "DS": "input/ds.csv"},
        "intermediates": [
            dict(
                {
                    "id": "EOT",
                    "dataset": "DS",
                    "key": ["USUBJID"],
                    "no_match": None,
                    "verifications": verifications,
                },
                **decl,
            )
        ],
        "output": {"path": "adsl.csv", "columns": ["USUBJID", "X"]},
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "DM.USUBJID"},
            {"name": "X", "type": "str", "derivation": column},
        ],
    }


def test_intermediate_unique_mapping_form_fails_at_its_check(tmp_path):
    spec = donor_spec(
        [
            {"unique": {"columns": ["DSCAT"], "id": "one-category"}},
            {"unique": {"columns": ["USUBJID"], "id": "donor-key"}},
        ],
        filter="DS.USUBJID = 'S2'",
    )
    ok = run(tmp_path, spec, {"dm.csv": DM, "ds.csv": DS})
    assert ok.splitlines() == ["USUBJID,X", "S1,", "S2,END"]
    spec["intermediates"][0].pop("filter")
    e = expect_error(tmp_path, spec, {"dm.csv": DM, "ds.csv": DS})
    assert pinned(e) == ("verification", "duplicate_intermediate_records", "REQ-1245")
    assert e.spec_paths == ["intermediates[0].verifications[0].unique"]


def test_input_intermediate_check_runs_before_any_row(tmp_path):
    # REQ-1245: the check holds whether or not a row reads the intermediate.
    spec = donor_spec([{"unique": ["USUBJID"]}], column={"literal": "Y"})
    e = expect_error(tmp_path, spec, {"dm.csv": DM, "ds.csv": DS})
    assert pinned(e) == ("verification", "duplicate_intermediate_records", "REQ-1245")
    assert e.context["duplicate_count"] == 1


def test_intermediate_duplicate_verification_id_fails(tmp_path):
    # REQ-0398: ids are unique within one intermediate's list.
    check = {"unique": {"columns": ["USUBJID"], "id": "donor-key"}}
    e = expect_error(tmp_path, donor_spec([check, check]), {"dm.csv": DM, "ds.csv": DS})
    assert pinned(e) == ("validation", "duplicate_identifier", "REQ-0398")
    assert e.spec_paths == ["intermediates[0].verifications[1].unique.id"]


@pytest.mark.parametrize(
    ("unique", "path"),
    [
        (["USUBJID", "NOPE"], "intermediates[0].verifications[0].unique[1]"),
        ({"columns": ["NOPE"]}, "intermediates[0].verifications[0].unique.columns[0]"),
    ],
)
def test_intermediate_unique_names_known_columns(tmp_path, unique, path):
    e = expect_error(
        tmp_path, donor_spec([{"unique": unique}]), {"dm.csv": DM, "ds.csv": DS}
    )
    assert pinned(e) == ("validation", "unknown_field", "REQ-1245")
    assert e.spec_paths == [path]


def test_intermediate_correlated_filter_cannot_combine_with_a_check(tmp_path):
    spec = donor_spec([{"unique": ["USUBJID"]}], filter="DS.USUBJID = DM.USUBJID")
    e = expect_error(tmp_path, spec, {"dm.csv": DM, "ds.csv": DS})
    assert e.condition == "correlated_filter_with_unique_verification"


def test_retired_intermediate_verification_field(tmp_path):
    spec = donor_spec(None)
    spec["intermediates"][0].pop("verifications")
    spec["intermediates"][0]["verification"] = {"unique": ["USUBJID"]}
    e = expect_error(tmp_path, spec, {"dm.csv": DM, "ds.csv": DS})
    assert pinned(e) == ("validation", "unknown_field", "REQ-0285")
    assert e.spec_paths == ["intermediates[0].verification"]


# -- dataset and column verifications (REQ-0374/0381/0398/0402) ---------------

LB = "USUBJID,SEQ,BL\nS1,1,Y\nS1,2,Y\nS2,1,\n"


def lb_spec(verifications, column_verifications=None):
    bl = {"name": "BL", "type": "str", "derivation": "LB.BL"}
    if column_verifications is not None:
        bl["verifications"] = column_verifications
    return {
        "schema_version": "1.0",
        "domain": "ADLB",
        "keys": ["USUBJID", "SEQ"],
        "input": {"LB": {"path": "input/lb.csv", "types": {"SEQ": "int"}}},
        "output": {"path": "adlb.csv", "columns": ["USUBJID", "SEQ", "BL"]},
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "LB.USUBJID"},
            {"name": "SEQ", "type": "int", "derivation": "LB.SEQ"},
            bl,
        ],
        "verifications": verifications,
    }


def test_unnamed_unique_form(tmp_path):
    ok = lb_spec([{"unique": ["USUBJID", "SEQ"]}])
    assert run(tmp_path, ok, {"lb.csv": LB}).count("\n") == 4
    e = expect_error(tmp_path, lb_spec([{"unique": ["USUBJID"]}]), {"lb.csv": LB})
    assert pinned(e) == ("verification", "unique_failed", "REQ-0381")
    assert e.spec_paths == ["verifications[0].unique"]


def test_ids_are_optional(tmp_path):
    # REQ-0374: assert, implies, and all_or_none name themselves only when
    # an id helps; unnamed, they still run.
    checks = [
        {"assert": {"expr": "SEQ >= 1"}},
        {"implies": {"when": "BL = 'Y'", "then": "SEQ >= 1"}},
        {"all_or_none": {"columns": ["USUBJID", "SEQ"]}},
    ]
    assert run(tmp_path, lb_spec(checks), {"lb.csv": LB}).count("\n") == 4
    e = expect_error(
        tmp_path, lb_spec([{"assert": {"expr": "SEQ = 1"}}]), {"lb.csv": LB}
    )
    assert pinned(e) == ("verification", "assert_failed", "REQ-0384")


def test_duplicate_ids_fail_only_within_one_list(tmp_path):
    # REQ-0398: a column's list and the dataset list may reuse an id.
    named = {"not_missing": {"id": "bl"}}
    shared = lb_spec(
        [{"row_count": {"id": "bl", "min": 1}}],
        column_verifications=[{"allowed_values": {"id": "bl", "values": ["Y"]}}],
    )
    assert run(tmp_path, shared, {"lb.csv": LB}).count("\n") == 4
    e = expect_error(tmp_path, lb_spec([], [named, named]), {"lb.csv": LB})
    assert pinned(e) == ("validation", "duplicate_identifier", "REQ-0398")
    assert e.spec_paths == ["columns.BL.verifications[1].not_missing.id"]


def test_unnamed_grouped_row_count_reports_each_failing_group(tmp_path):
    # REQ-0402: valid without an id; the report names the groups and counts.
    count = {"group_by": ["USUBJID"], "filter": "BL = 'Y'", "min": 1, "max": 1}
    e = expect_error(tmp_path, lb_spec([{"row_count": count}]), {"lb.csv": LB})
    assert pinned(e) == ("verification", "row_count_failed", "REQ-0385")
    assert e.context["keys"] == [{"USUBJID": "S1"}, {"USUBJID": "S2"}]
    assert e.context["counts"] == [2, 0]
    assert "verification_id" not in e.context
