"""A named intermediate as a row template's driver (REQ-1262).

`adam-adae-serious-events` pins the plain case; these pin the edges it
leaves open: derived fields and the `columns` projection, a column
derivation reading the driver, the eligibility rules, and the
determinable-type rule for exposed derived fields.
"""

import os

import pytest
import yaml

from yamaa import YamaaError, derive

AE = "USUBJID,AESEQ,AESER,AESEV\nS1,1,Y,MILD\nS1,2,N,MILD\nS2,1,Y,SEVERE\nS2,2,Y,MILD\n"


def run(tmp_path, spec):
    d = str(tmp_path)
    os.makedirs(os.path.join(d, "input"), exist_ok=True)
    with open(os.path.join(d, "input", "ae.csv"), "w", encoding="utf-8") as f:
        f.write(AE)
    spec_path = os.path.join(d, "spec.yaml")
    with open(spec_path, "w", encoding="utf-8") as f:
        yaml.safe_dump(spec, f)
    return derive(spec_path)


def expect_error(tmp_path, spec):
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, spec)
    return ei.value


def driver_spec(intermediate, derivations=None, columns=None):
    spec = {
        "schema_version": "1.0",
        "domain": "ADAE",
        "keys": ["USUBJID", "AESEQ"],
        "input": {"AE": {"path": "input/ae.csv", "types": {"AESEQ": "int"}}},
        "output": {"path": "adae.csv", "columns": ["USUBJID", "AESEQ", "N"]},
        "intermediates": [{"id": "SER", "dataset": "AE", **intermediate}],
        "columns": columns
        or [
            {"name": "USUBJID", "type": "str"},
            {"name": "AESEQ", "type": "int"},
            {"name": "N", "type": "int"},
        ],
        "rows": [
            {
                "id": "serious",
                "dataset": "SER",
                "derivations": derivations
                or {"USUBJID": "SER.USUBJID", "AESEQ": "SER.AESEQ", "N": "SER.N"},
            }
        ],
    }
    return spec


NUMBERED = {
    "filter": "AE.AESER = 'Y'",
    "derivations": {
        "N": {
            "row_number": {"window": {"group_by": ["USUBJID"], "order_by": ["AESEQ"]}}
        },
    },
}


def test_a_driver_exposes_its_derived_fields_in_source_order(tmp_path):
    # The row_number runs over the filtered records, and the retained
    # records drive one row each, in stored order.
    out = run(tmp_path, driver_spec(NUMBERED))
    assert out.splitlines() == ["USUBJID,AESEQ,N", "S1,1,1", "S2,1,1", "S2,2,2"]


def test_a_column_derivation_reads_the_driver_record(tmp_path):
    # The driver's qualifier reads the row's own driver record in the
    # column phase too, never a keyed lookup.
    spec = driver_spec(
        NUMBERED,
        derivations={"USUBJID": "SER.USUBJID", "AESEQ": "SER.AESEQ"},
        columns=[
            {"name": "USUBJID", "type": "str"},
            {"name": "AESEQ", "type": "int"},
            {"name": "N", "type": "int", "derivation": "SER.N"},
        ],
    )
    out = run(tmp_path, spec)
    assert out.splitlines() == ["USUBJID,AESEQ,N", "S1,1,1", "S2,1,1", "S2,2,2"]


def test_columns_limits_what_the_template_may_read(tmp_path):
    spec = driver_spec({**NUMBERED, "columns": ["USUBJID", "AESEQ"]})
    err = expect_error(tmp_path, spec)
    assert (err.phase, err.condition) == ("validation", "unknown_field")


@pytest.mark.parametrize(
    "declared",
    [
        {"key": ["USUBJID"]},
        {"order_by": ["AE.AESEQ"], "keep": "first"},
        {"no_match": None},
    ],
)
def test_a_matching_or_selecting_intermediate_cannot_drive(tmp_path, declared):
    err = expect_error(tmp_path, driver_spec({**NUMBERED, **declared}))
    assert (err.phase, err.condition, err.requirement) == (
        "validation",
        "invalid_intermediate_driver",
        "REQ-1262",
    )
    assert err.spec_paths == ["rows.serious.dataset"]


def test_a_driver_filter_may_not_read_a_current_row(tmp_path):
    # A filter correlated with the base's current record is not source-only.
    spec = driver_spec({**NUMBERED, "filter": "AE.AESER = 'Y' AND AE2.AESEQ > 1"})
    spec["input"]["AE2"] = {"path": "input/ae.csv", "types": {"AESEQ": "int"}}
    spec["base"] = "AE2"
    err = expect_error(tmp_path, spec)
    assert err.condition == "invalid_intermediate_driver"


def test_an_exposed_derived_field_needs_a_determinable_type(tmp_path):
    untyped = {
        "filter": "AE.AESER = 'Y'",
        "derivations": {"N": {"compute": {"expr": "AE.AESEQ * 2"}}},
    }
    err = expect_error(tmp_path, driver_spec(untyped))
    assert (err.condition, err.requirement) == (
        "unknown_intermediate_driver_type",
        "REQ-1262",
    )
    assert err.context["field"] == "N"


def test_a_case_takes_the_type_its_present_branches_share(tmp_path):
    # A missing branch does not decide the type; the remaining integer
    # literals do.
    typed = {
        "filter": "AE.AESER = 'Y'",
        "derivations": {
            "N": {
                "case": [
                    {"when": "AE.AESEV = 'SEVERE'", "then": {"literal": 3}},
                    {"when": "AE.AESEV = 'MILD'", "then": {"literal": 1}},
                    {"otherwise": {"literal": None}},
                ]
            }
        },
    }
    out = run(tmp_path, driver_spec(typed))
    assert out.splitlines() == ["USUBJID,AESEQ,N", "S1,1,1", "S2,1,3", "S2,2,1"]
