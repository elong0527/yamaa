"""Boundary tests for the 2026-09-25 drift catch-up.

Covers SELF named intermediates (REQ-0120/0136) and the REQ-1260
row-default promotion: donor pools per template, correlated filters
evaluated per driver row, uniqueness over the completed pool, and the
validation gates (SELF needs rows; SELF is never a qualified source).
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


AE = (
    "USUBJID,AESEQ,AEDECOD\n"
    "S1,1,HEADACHE\n"
    "S1,2,NAUSEA\n"
    "S1,3,FATIGUE\n"
    "S2,1,COUGH\n"
    "S2,2,FEVER\n"
    "S2,3,MYALGIA\n"
    "S2,4,RASH\n"
)


def self_spec(**kw):
    spec = {
        "schema_version": "1.0",
        "domain": "ADAE",
        "keys": ["USUBJID", "AESEQ"],
        "input": {"AE": {"path": "input/ae.csv", "types": {"AESEQ": "int"}}},
        "output": {
            "path": "adae.csv",
            "columns": ["USUBJID", "AESEQ", "AEDECOD", "PREV"],
        },
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "AE.USUBJID"},
            {"name": "AESEQ", "type": "int", "derivation": "AE.AESEQ"},
            {"name": "AEDECOD", "type": "str", "derivation": "AE.AEDECOD"},
            {"name": "PREV", "type": "str"},
        ],
        "intermediates": [
            {
                "id": "prev-ae",
                "dataset": "SELF",
                "key": ["USUBJID"],
                "filter": "SELF.AESEQ < AE.AESEQ",
                "order_by": [{"variable": "SELF.AESEQ", "direction": "desc"}],
                "keep": "first",
            },
        ],
        "rows": [
            {
                "id": "first",
                "dataset": "AE",
                "filter": (
                    "(AE.USUBJID = 'S1' AND AE.AESEQ <= 2) OR "
                    "(AE.USUBJID = 'S2' AND AE.AESEQ <= 3)"
                ),
                "derivations": {"PREV": {"literal": None}},
            },
            {
                "id": "rest",
                "dataset": "AE",
                "filter": (
                    "(AE.USUBJID = 'S1' AND AE.AESEQ = 3) OR "
                    "(AE.USUBJID = 'S2' AND AE.AESEQ = 4)"
                ),
                "derivations": {"PREV": "prev-ae.AEDECOD"},
            },
        ],
    }
    spec.update(kw.get("root", {}))
    if "intermediates" in kw:
        spec["intermediates"] = kw["intermediates"]
    if "rows" in kw:
        spec["rows"] = kw["rows"]
    return spec


# -- donor pool and REQ-1260 promotion ---------------------------------------


def test_self_read_sees_only_earlier_templates(tmp_path):
    # REQ-0120/0136: during the second template's construction the donor
    # pool is the first template's completed rows only. S1 seq 3 sees
    # donors {1, 2} and takes the latest (NAUSEA); S2 seq 4 sees
    # donors {1, 2, 3} and takes the latest (MYALGIA).
    got = run(tmp_path, self_spec(), {"ae.csv": AE})
    assert got == (
        "USUBJID,AESEQ,AEDECOD,PREV\n"
        "S1,1,HEADACHE,\n"
        "S1,2,NAUSEA,\n"
        "S2,1,COUGH,\n"
        "S2,2,FEVER,\n"
        "S2,3,MYALGIA,\n"
        "S1,3,FATIGUE,NAUSEA\n"
        "S2,4,RASH,MYALGIA\n"
    )


def test_self_correlated_filter_cached_per_driver_row(tmp_path):
    # The row-phase lookup cache must not collapse driver rows that
    # share match values but differ in the correlated filter's driver
    # fields: S1 seq 2 and seq 3 share (USUBJID,) but match different
    # donors.
    from yamaa.engine import Engine, _RowCtx

    d = str(tmp_path)
    os.makedirs(os.path.join(d, "input"), exist_ok=True)
    with open(os.path.join(d, "input", "ae.csv"), "w", encoding="utf-8") as f:
        f.write(AE)
    spec_path = os.path.join(d, "spec.yaml")
    with open(spec_path, "w", encoding="utf-8") as f:
        yaml.safe_dump(self_spec(), f)
    e = Engine(spec_path)
    e.rows, e._origins, e._recs, e._derived = [], [], [], []
    e._lookups, e._eligible, e._self_marks = {}, {}, []
    e._row_phase = True
    e._build_template(e.templates[0])
    e._self_marks.append(len(e.rows))
    e._eligible, e._lookups = {}, {}
    decl = e.lookups_decl["prev-ae"]
    t = e.templates[1]
    rec2 = {"USUBJID": "S1", "AESEQ": 2, "AEDECOD": "NAUSEA"}
    rec3 = {"USUBJID": "S1", "AESEQ": 3, "AEDECOD": "FATIGUE"}
    m2 = _RowCtx(e, "row", t, "AE", record=rec2)._match_lookup_row(
        decl, ["USUBJID"], ["S1"]
    )
    m3 = _RowCtx(e, "row", t, "AE", record=rec3)._match_lookup_row(
        decl, ["USUBJID"], ["S1"]
    )
    assert m2["AEDECOD"] == "HEADACHE"
    assert m3["AEDECOD"] == "NAUSEA"


def test_self_read_in_first_template_sees_no_donors(tmp_path):
    # REQ-0136: rows currently being constructed are not eligible donors,
    # so a SELF read in the first template finds nothing.
    spec = self_spec()
    spec["rows"][0]["derivations"] = {
        "PREV": "prev-ae.AEDECOD",
    }
    got = run(tmp_path, spec, {"ae.csv": AE})
    lines = got.strip().split("\n")
    assert lines[1] == "S1,1,HEADACHE,"
    assert lines[2] == "S1,2,NAUSEA,"
    assert lines[3] == "S2,1,COUGH,"
    assert lines[4] == "S2,2,FEVER,"


def test_self_unique_runs_over_completed_pool(tmp_path):
    # REQ-0120/1245: verification.unique fails when the completed donor
    # pool holds a repeated combination. (A donor-only filter keeps the
    # intermediate clear of the REQ-1245 correlated-filter ban.)
    spec = self_spec()
    spec["intermediates"].append(
        {
            "id": "serious-ae",
            "dataset": "SELF",
            "key": ["USUBJID"],
            "filter": "SELF.AESEQ <= 2",
            "verification": {"unique": ["USUBJID"]},
        },
    )
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, spec, {"ae.csv": AE})
    assert ei.value.phase == "verification"
    assert ei.value.condition == "duplicate_intermediate_records"


# -- validation gates --------------------------------------------------------


def test_self_without_rows_is_rejected(tmp_path):
    # REQ-0120: dataset SELF needs row templates.
    spec = self_spec(rows=[])
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, spec, {"ae.csv": AE})
    assert ei.value.phase == "validation"
    assert ei.value.requirement == "REQ-0120"


def test_self_direct_read_is_rejected(tmp_path):
    # REQ-0120: SELF is never a qualified source in a rows derivation.
    spec = self_spec()
    spec["rows"][1]["derivations"] = {"PREV": "SELF.AEDECOD"}
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, spec, {"ae.csv": AE})
    assert ei.value.phase == "validation"
    assert ei.value.condition == "prohibited_construct"


def test_self_filter_unknown_donor_field_rejected(tmp_path):
    # REQ-0133: a SELF filter naming a non-donor field fails.
    spec = self_spec()
    spec["intermediates"][0]["filter"] = "SELF.NOPE < AE.AESEQ"
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, spec, {"ae.csv": AE})
    assert ei.value.phase == "validation"
    assert ei.value.condition == "unknown_field"


def test_self_key_unknown_donor_field_rejected(tmp_path):
    # R003-6: a SELF key must name a donor field.
    spec = self_spec()
    spec["intermediates"][0]["key"] = ["USUBJID", "NOPE"]
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, spec, {"ae.csv": AE})
    assert ei.value.phase == "validation"
    assert ei.value.condition == "unknown_field"
