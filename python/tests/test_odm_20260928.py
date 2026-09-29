"""Boundary tests for ODM item reads (REQ-1265..REQ-1278).

The benchmark corpus pins the headline conditions; these pin the edges it
leaves open: Parquet ODM inputs and their vendor fields, a read's scope and
its narrowing by `form` and `filter`, the phase an unresolved read fails in,
and where a read has no scope at all.
"""

import os

import pyarrow as pa
import pyarrow.parquet as pq
import pytest
import yaml

from yamaa import YamaaError, derive


def run(tmp_path, spec, inputs):
    d = str(tmp_path)
    os.makedirs(os.path.join(d, "input"), exist_ok=True)
    for name, rows in inputs.items():
        full = os.path.join(d, "input", name)
        if isinstance(rows, pa.Table):
            pq.write_table(rows, full)
            continue
        with open(full, "w", encoding="utf-8") as f:
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


# -- ODM item reads ----------------------------------------------------------

HEADER = (
    "StudyOID,MetaDataVersionOID,SubjectKey,StudyEventOID,StudyEventRepeatKey,"
    "FormOID,FormRepeatKey,ItemGroupOID,ItemGroupRepeatKey,ItemOID,Value"
)


def odm_csv(*records):
    return "\n".join([HEADER, *records]) + "\n"


ODM = odm_csv(
    "S,M,001,SCR,1,FO.DM,1,IG.DM,1,IT.SEX,F",
    "S,M,001,SCR,1,FO.VS,1,IG.VS,1,IT.WT,61",
    "S,M,001,SCR,1,FO.VS,1,IG.VS,2,IT.WT,62",
    "S,M,002,SCR,1,FO.DM,1,IG.DM,1,IT.SEX,M",
)


def subject_spec(derivation, rows=None, **extra):
    spec = {
        "schema_version": "1.0",
        "domain": "DM",
        "keys": ["USUBJID"],
        "input": {"ODM": "input/odm.csv"},
        "output": {"path": "dm.csv", "columns": ["USUBJID", "X"]},
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "ODM.SubjectKey"},
            {"name": "X", "type": "str", "derivation": derivation},
        ],
    }
    if rows is not None:
        spec["rows"] = rows
    spec.update(extra)
    return spec


def test_odm_read_narrows_by_form_and_filter(tmp_path):
    # REQ-1271: `form` and `filter` narrow the item's records in the scope.
    spec = subject_spec(
        {
            "odm": {
                "item": "ODM.IT.WT",
                "form": ["FO.VS"],
                "filter": "ODM.ItemGroupRepeatKey = '2'",
            }
        }
    )
    out = run(tmp_path, spec, {"odm.csv": ODM})
    assert out.splitlines() == ["USUBJID,X", "001,62", "002,"]


def test_odm_read_without_narrowing_counts_records_not_values(tmp_path):
    # REQ-1272/REQ-1278: the two weight records differ on their item group
    # repeat, so the read identifies two records and fails.
    e = expect_error(tmp_path, subject_spec({"odm": "ODM.IT.WT"}), {"odm.csv": ODM})
    assert pinned(e) == ("derivation", "odm_not_unique", "REQ-1278")
    assert e.spec_paths == ["columns.X.derivation.odm"]
    assert e.context["differ"] == {"ItemGroupRepeatKey": ["1", "2"]}
    assert e.context["scope"] == {"USUBJID": "001"}


def test_odm_read_in_grouped_row_fails_at_row_construction(tmp_path):
    # REQ-1269: the grouped scope is the records equal on the hierarchy
    # fields of `group_by`; both weight records share it.
    rows = [
        {
            "id": "subject",
            "group_by": ["ODM.SubjectKey"],
            "derivations": {"X": {"odm": "ODM.IT.WT"}},
        }
    ]
    spec = subject_spec(None, rows=rows)
    spec["columns"][1].pop("derivation")
    e = expect_error(tmp_path, spec, {"odm.csv": ODM})
    assert pinned(e) == ("row_construction", "odm_not_unique", "REQ-1278")
    assert e.spec_paths == ["rows.subject.derivations.X.odm"]
    assert e.context["row"] == "subject"


def test_odm_record_driven_row_reads_its_own_item_group(tmp_path):
    # REQ-1269: a record-driven row's scope is its driver record's item
    # group occurrence, so each weight row reads the weight beside it.
    rows = [
        {
            "id": "weight",
            "filter": "ODM.ItemOID = 'IT.WT'",
            "derivations": {"SEQ": "ODM.ItemGroupRepeatKey"},
        }
    ]
    spec = subject_spec({"odm": "ODM.IT.WT"}, rows=rows)
    spec["keys"] = ["USUBJID", "SEQ"]
    spec["columns"].append({"name": "SEQ", "type": "int"})
    spec["output"]["columns"] = ["USUBJID", "SEQ", "X"]
    out = run(tmp_path, spec, {"odm.csv": ODM})
    assert out.splitlines() == ["USUBJID,SEQ,X", "001,1,61", "001,2,62"]


def test_odm_read_in_intermediate_has_no_scope(tmp_path):
    # REQ-1270/REQ-1277: a named intermediate's record is not a row.
    spec = subject_spec(
        "LOOK.W",
        intermediates=[
            {
                "id": "LOOK",
                "dataset": "ODM",
                "key": {"SubjectKey": "USUBJID"},
                "filter": "ODM.ItemOID = 'IT.SEX'",
                "derivations": {"W": {"odm": "ODM.IT.WT"}},
            }
        ],
    )
    e = expect_error(tmp_path, spec, {"odm.csv": ODM})
    assert pinned(e) == ("validation", "invalid_odm_context", "REQ-1277")
    assert e.spec_paths == ["intermediates[0].derivations.W.odm"]
    assert e.context == {"dataset": "ODM", "location": "intermediate"}


def test_odm_read_in_aggregate_derive_has_no_scope(tmp_path):
    # REQ-1270: an aggregate's derive step evaluates per record of the
    # aggregated relation, which is no row.
    agg = {
        "aggregate": {
            "key": ["SubjectKey"],
            "derive": [
                {"name": "W", "type": "str", "derivation": {"odm": "ODM.IT.WT"}}
            ],
            "expr": "MAX(W)",
        }
    }
    e = expect_error(tmp_path, subject_spec(agg), {"odm.csv": ODM})
    assert pinned(e) == ("validation", "invalid_odm_context", "REQ-1277")
    assert e.context["location"] == "derive"


def test_odm_column_read_needs_only_the_templates_that_evaluate_it(tmp_path):
    # REQ-1260/REQ-1270: an `odm` column derivation is row-local. The roster
    # template derives X itself, so only the ODM template evaluates the
    # column's `odm` read and needs a scope (REQ-1277).
    roster = "USUBJID\n003\n"
    rows = [
        {
            "id": "collected",
            "dataset": "ODM",
            "group_by": ["ODM.SubjectKey"],
            "derivations": {"USUBJID": "ODM.SubjectKey"},
        },
        {
            "id": "listed",
            "dataset": "ROSTER",
            "derivations": {"USUBJID": "ROSTER.USUBJID", "X": {"literal": "U"}},
        },
    ]
    spec = subject_spec({"odm": "ODM.IT.SEX"}, rows=rows)
    spec["input"]["ROSTER"] = "input/roster.csv"
    spec["columns"][0].pop("derivation")
    out = run(tmp_path, spec, {"odm.csv": ODM, "roster.csv": roster})
    assert out.splitlines() == ["USUBJID,X", "001,F", "002,M", "003,U"]
    rows[1]["derivations"].pop("X")
    e = expect_error(tmp_path, spec, {"odm.csv": ODM, "roster.csv": roster})
    assert pinned(e) == ("validation", "invalid_odm_context", "REQ-1277")


def test_a_template_may_not_override_a_column_phase_derivation(tmp_path):
    # REQ-1260: only a row-local column derivation is a default a template
    # may override; an aggregate stays in the column phase.
    rows = [
        {
            "id": "collected",
            "dataset": "ODM",
            "group_by": ["ODM.SubjectKey"],
            "derivations": {"USUBJID": "ODM.SubjectKey", "X": {"literal": "U"}},
        }
    ]
    spec = subject_spec({"aggregate": {"expr": "COUNT(ODM.Value)"}}, rows=rows)
    spec["columns"][0].pop("derivation")
    e = expect_error(tmp_path, spec, {"odm.csv": ODM})
    assert pinned(e) == ("validation", "duplicate_derivation", "REQ-1260")
    assert e.context == {"column": "X", "rows": ["collected"]}


def test_odm_filter_names_only_schema_fields(tmp_path):
    # REQ-1271: a vendor field is not a field of the ODM input.
    spec = subject_spec({"odm": {"item": "ODM.IT.SEX", "filter": "ODM.SITE = 'A'"}})
    e = expect_error(tmp_path, spec, {"odm.csv": ODM})
    assert pinned(e) == ("validation", "unknown_field", "REQ-1271")
    assert e.spec_paths == ["columns.X.derivation.odm.filter"]


def test_odm_item_names_a_declared_input(tmp_path):
    e = expect_error(tmp_path, subject_spec({"odm": "EDC.IT.SEX"}), {"odm.csv": ODM})
    assert pinned(e) == ("validation", "unknown_field", "REQ-0103")
    assert e.spec_paths == ["columns.X.derivation.odm.item"]


def test_odm_item_must_carry_an_item_oid(tmp_path):
    e = expect_error(tmp_path, subject_spec({"odm": "ODM"}), {"odm.csv": ODM})
    assert pinned(e) == ("validation", "invalid_field_type", "REQ-0287")
    assert e.spec_paths == ["columns.X.derivation.odm.item"]


def test_odm_input_hides_vendor_fields(tmp_path):
    # REQ-1267: stored names bind by ASCII case folding; a vendor field is
    # not exposed to any read.
    odm = ODM.replace("SubjectKey", "SUBJECTKEY").replace("Value", "value")
    odm = odm.replace("\n", ",V\n")  # a vendor field V on every record
    spec = subject_spec({"odm": "ODM.IT.SEX"})
    out = run(tmp_path, spec, {"odm.csv": odm})
    assert out.splitlines() == ["USUBJID,X", "001,F", "002,M"]
    spec["columns"].append({"name": "V", "type": "str", "derivation": "ODM.V"})
    e = expect_error(tmp_path, spec, {"odm.csv": odm})
    assert pinned(e) == ("validation", "unknown_field", "REQ-0103")


def odm_parquet(value=None, vendor=None):
    """One ODM record as Parquet; `value` and `vendor` are (type, cell)."""
    cells = ["S", "M", "001", "SCR", "1", "FO.DM", "1", "IG.DM", "1", "IT.SEX"]
    fields = [(name, pa.string(), cell) for name, cell in zip(HEADER.split(","), cells)]
    fields.append(("Value", *(value or (pa.string(), "F"))))
    if vendor is not None:
        fields.append(("AUDIT", *vendor))
    return pa.Table.from_arrays(
        [pa.array([cell], kind) for _, kind, cell in fields],
        schema=pa.schema([pa.field(name, kind) for name, kind, _ in fields]),
    )


def test_odm_parquet_vendor_field_is_never_typed(tmp_path):
    # REQ-1267/REQ-1268: only the bound fields are read, so a vendor field
    # outside the closed type mapping does not stop the run.
    spec = subject_spec({"odm": "ODM.IT.SEX"})
    spec["input"]["ODM"] = "input/odm.parquet"
    out = run(tmp_path, spec, {"odm.parquet": odm_parquet(vendor=(pa.int8(), 1))})
    assert out.splitlines() == ["USUBJID,X", "001,F"]


def test_odm_parquet_bound_field_must_be_text(tmp_path):
    # REQ-1268/REQ-1275: a schema field stored as anything but text fails
    # at validation, naming the field.
    spec = subject_spec({"odm": "ODM.IT.SEX"})
    spec["input"]["ODM"] = "input/odm.parquet"
    e = expect_error(
        tmp_path, spec, {"odm.parquet": odm_parquet(value=(pa.int64(), 34))}
    )
    assert pinned(e) == ("validation", "odm_schema_field_type", "REQ-1275")
    assert e.spec_paths == ["input.ODM.path"]
    assert e.context["field"] == "Value"
    assert e.context["stored_type"] == "int"
