"""Boundary tests for the 2026-09-24 drift catch-up.

Covers the normative changes that landed after the clean-room's last
sync: named windows (REQ-1251/1252/1253), the date_impute month policy
(REQ-0592), rename-only intermediates (REQ-1248), cut missing input
(REQ-0334), row-construction inline lookups (REQ-0126), row catalogs
(REQ-1249), and the mapping unmapped result (REQ-1110).
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


def base_spec(**kw):
    spec = {
        "schema_version": "1.0",
        "domain": "ADSL",
        "keys": ["USUBJID"],
        "input": {"DM": {"path": "input/dm.csv", "types": kw.get("types", {})}},
        "output": {"path": "adsl.csv", "columns": ["USUBJID", "X"]},
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "DM.USUBJID"},
            {
                "name": "X",
                "type": kw.get("xtype", "str"),
                "derivation": kw["derivation"],
            },
        ],
    }
    spec.update(kw.get("root", {}))
    return spec


DM = "USUBJID,VAL\nS1,10\nS2,20\n"


# -- named windows (REQ-1251/1252/1253) -------------------------------------


def test_named_window_expands_before_validation(tmp_path):
    # REQ-1251/1252: a string window reference is replaced by an
    # independent copy of the named definition.
    spec = base_spec(
        derivation={"rank": {"source": "DM.VAL", "window": "by_val"}},
        root={"windows": {"by_val": {"order_by": ["DM.VAL"]}}},
    )
    out = run(tmp_path, spec, {"dm.csv": DM})
    lines = out.splitlines()
    assert lines[1].endswith(",1")
    assert lines[2].endswith(",2")


def test_unknown_window_reference_fails(tmp_path):
    # REQ-1253: a reference to an undeclared name fails unknown_window at
    # the expression's window field.
    spec = base_spec(derivation={"rank": {"source": "DM.VAL", "window": "nope"}})
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, spec, {"dm.csv": DM})
    e = ei.value
    assert (e.phase, e.condition, e.requirement) == (
        "validation",
        "unknown_window",
        "REQ-1253",
    )
    assert e.spec_paths == ["columns.X.derivation.rank.window"]


def test_named_window_definition_shape_is_validated(tmp_path):
    # REQ-1251: even an unused definition with a bad shape fails.
    spec = base_spec(
        derivation="DM.VAL",
        root={"windows": {"bad": {"group_by": "DM.USUBJID"}}},
    )
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, spec, {"dm.csv": DM})
    e = ei.value
    assert (e.phase, e.condition) == ("validation", "invalid_field_type")
    assert e.spec_paths == ["windows.bad.group_by"]


# -- date_impute month policy (REQ-0592) ------------------------------------


def _impute_spec(**kw):
    payload = {"source": "DM.DTC", "day": 15}
    payload.update(kw)
    return base_spec(derivation={"date_impute": payload})


def test_date_impute_month_required_for_year_precision(tmp_path):
    # REQ-0592: with minimum_source_precision year (the default), month
    # is required.
    spec = _impute_spec()
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, spec, {"dm.csv": "USUBJID,DTC\nS1,2020\n"})
    e = ei.value
    assert (e.phase, e.condition, e.requirement) == (
        "validation",
        "month_required",
        "REQ-0592",
    )
    assert e.spec_paths == ["columns.X.derivation.date_impute.month"]


def test_date_impute_month_not_permitted_for_month_precision(tmp_path):
    # REQ-0592: with minimum_source_precision month, month must be
    # absent.
    spec = _impute_spec(minimum_source_precision="month", month=6)
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, spec, {"dm.csv": "USUBJID,DTC\nS1,2020-05\n"})
    e = ei.value
    assert (e.phase, e.condition, e.requirement) == (
        "validation",
        "month_not_permitted",
        "REQ-0592",
    )


# -- rename-only intermediates (REQ-1248) -----------------------------------


def test_rename_only_intermediate_fails(tmp_path):
    # REQ-1248: an intermediate declaring only id and dataset fails.
    spec = base_spec(derivation="DM.USUBJID")
    spec["intermediates"] = [{"id": "DM2", "dataset": "DM"}]
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, spec, {"dm.csv": DM})
    e = ei.value
    assert (e.phase, e.condition, e.requirement) == (
        "validation",
        "rename_only_intermediate",
        "REQ-1248",
    )


def test_intermediate_with_filter_is_not_rename_only(tmp_path):
    spec = base_spec(derivation="DM.USUBJID")
    spec["intermediates"] = [
        {"id": "DM2", "dataset": "DM", "filter": "DM.USUBJID = 'S1'"}
    ]
    out = run(tmp_path, spec, {"dm.csv": DM})
    assert out is not None


# -- cut missing input (REQ-0334) -------------------------------------------


def test_cut_missing_input_without_handler_fails(tmp_path):
    # REQ-0334: a missing cut input with no `missing` handler fails in
    # the cut phase.
    spec = base_spec(
        derivation={
            "cut": {"source": "DM.VAL", "breaks": [15], "labels": ["low", "high"]}
        },
        types={"VAL": "int"},
    )
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, spec, {"dm.csv": "USUBJID,VAL\nS1,\n"})
    e = ei.value
    assert (e.phase, e.condition, e.requirement) == ("cut", "missing_input", "REQ-0334")


def test_cut_missing_handler_still_answers(tmp_path):
    spec = base_spec(
        derivation={
            "cut": {
                "source": "DM.VAL",
                "breaks": [15],
                "labels": ["low", "high"],
                "missing": "unknown",
            }
        },
        types={"VAL": "int"},
    )
    out = run(tmp_path, spec, {"dm.csv": "USUBJID,VAL\nS1,\nS2,20\n"})
    lines = out.splitlines()
    assert lines[1].endswith(",unknown")
    assert lines[2].endswith(",high")


# -- row-construction inline lookups (REQ-0126) ------------------------------


def _row_spec(derivations, rows_extra=None):
    spec = {
        "schema_version": "1.0",
        "domain": "DMX",
        "keys": ["USUBJID"],
        "input": {"DM": {"path": "input/dm.csv"}, "SUPP": {"path": "input/supp.csv"}},
        "output": {"path": "dmx.csv", "columns": ["USUBJID", "X"]},
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "DM.USUBJID"},
            {"name": "X", "type": "str"},
        ],
        "rows": [
            {"id": "dm", "dataset": "DM", "derivations": derivations},
        ],
    }
    if rows_extra:
        spec["rows"][0].update(rows_extra)
    return spec


def test_row_inline_lookup_reads_input_dataset(tmp_path):
    # REQ-0126: a row template may look up an input dataset when every
    # match variable is available during row construction.
    spec = _row_spec(
        {
            "USUBJID": "DM.USUBJID",
            "X": {"lookup": {"dataset": "SUPP", "key": "USUBJID", "value": "QVAL"}},
        }
    )
    inputs = {"dm.csv": "USUBJID\nS1\n", "supp.csv": "USUBJID,QVAL\nS1,hello\n"}
    out = run(tmp_path, spec, inputs)
    assert "S1,hello" in out.splitlines()


def test_row_inline_lookup_later_phase_value_fails(tmp_path):
    # REQ-0126: in a grouped template, a qualified match on a driver
    # field that is not a group key is a later-phase value and fails as
    # phase_boundary.
    spec = _row_spec(
        {
            "USUBJID": "DM.USUBJID",
            "X": {
                "lookup": {
                    "dataset": "SUPP",
                    "key": "USUBJID",
                    "key_base": ["DM.VAL"],
                    "value": "QVAL",
                }
            },
        },
        rows_extra={"group_by": ["DM.USUBJID"]},
    )
    inputs = {"dm.csv": "USUBJID,VAL\nS1,10\n", "supp.csv": "USUBJID,QVAL\nS1,hello\n"}
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, spec, inputs)
    e = ei.value
    assert (e.phase, e.condition) == ("row_construction", "phase_boundary")


def test_row_inline_lookup_grouped_group_key_match(tmp_path):
    # REQ-0126: grouped rows match on group keys.
    spec = _row_spec(
        {
            "USUBJID": "DM.USUBJID",
            "X": {"lookup": {"dataset": "SUPP", "key": "USUBJID", "value": "QVAL"}},
        },
        rows_extra={"group_by": ["DM.USUBJID"]},
    )
    inputs = {"dm.csv": "USUBJID\nS1\n", "supp.csv": "USUBJID,QVAL\nS1,hello\n"}
    out = run(tmp_path, spec, inputs)
    assert "S1,hello" in out.splitlines()


# -- row catalogs (REQ-1249) -------------------------------------------------


def _catalog_spec(tmp_path, csv_text, derivations, template_filter=None):
    d = str(tmp_path)
    os.makedirs(os.path.join(d, "input"), exist_ok=True)
    with open(os.path.join(d, "input", "tests.csv"), "w", encoding="utf-8") as f:
        f.write(csv_text)
    with open(os.path.join(d, "input", "dm.csv"), "w", encoding="utf-8") as f:
        f.write("USUBJID\nS1\nS2\n")
    template = {
        "id": "t",
        "dataset": "DM",
        "catalog": {
            "path": "input/tests.csv",
            "id_column": "ID",
            "types": {"N": "int"},
            "unique_columns": ["CODE"],
        },
        "derivations": derivations,
    }
    if template_filter is not None:
        template["filter"] = template_filter
    spec = {
        "schema_version": "1.0",
        "domain": "DMX",
        "keys": ["USUBJID"],
        "input": {"DM": {"path": "input/dm.csv"}},
        "output": {"path": "dmx.csv", "columns": ["USUBJID", "X"]},
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "DM.USUBJID"},
            {"name": "X", "type": "str"},
        ],
        "rows": [template],
    }
    spec_path = os.path.join(d, "spec.yaml")
    with open(spec_path, "w", encoding="utf-8") as f:
        yaml.safe_dump(spec, f)
    return derive(spec_path)


def test_catalog_expands_with_typed_values(tmp_path):
    # REQ-1249: one template per record in CSV order; an entire scalar
    # placeholder becomes the typed value (int 1/2 render as "1"/"2").
    # Each generated template keeps a distinct record via its filter so
    # output keys stay unique.
    out = _catalog_spec(
        tmp_path,
        "ID,CODE,N\nsysbp,S1,1\ndiabp,S2,2\n",
        {"USUBJID": "DM.USUBJID", "X": {"literal": "${N}"}},
        template_filter="DM.USUBJID = ${CODE}",
    )
    lines = out.splitlines()
    assert lines[1] == "S1,1"
    assert lines[2] == "S2,2"


def test_catalog_predicate_placeholder_is_quoted(tmp_path):
    # REQ-1249: inside a predicate filter the placeholder becomes a
    # quoted literal; a quote inside the value is escaped (the predicate
    # still parses) and matches nothing.
    d = str(tmp_path)
    os.makedirs(os.path.join(d, "input"), exist_ok=True)
    with open(os.path.join(d, "input", "tests.csv"), "w", encoding="utf-8") as f:
        f.write("ID,CODE\nkeep,S1\ndrop,S'X\n")
    with open(os.path.join(d, "input", "dm.csv"), "w", encoding="utf-8") as f:
        f.write("USUBJID\nS1\n")
    spec = {
        "schema_version": "1.0",
        "domain": "DMX",
        "keys": ["USUBJID"],
        "input": {"DM": {"path": "input/dm.csv"}},
        "output": {"path": "dmx.csv", "columns": ["USUBJID", "X"]},
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "DM.USUBJID"},
            {"name": "X", "type": "str"},
        ],
        "rows": [
            {
                "id": "t",
                "dataset": "DM",
                "catalog": {"path": "input/tests.csv", "id_column": "ID"},
                "filter": "DM.USUBJID = ${CODE}",
                "derivations": {"USUBJID": "DM.USUBJID", "X": {"literal": "${CODE}"}},
            },
        ],
    }
    spec_path = os.path.join(d, "spec.yaml")
    with open(spec_path, "w", encoding="utf-8") as f:
        yaml.safe_dump(spec, f)
    out = derive(spec_path)
    lines = out.splitlines()
    # the S1 record matches its filter; the S'X record parses (escaped
    # quote) but matches nothing and is filtered out
    assert lines[1] == "S1,S1"
    assert len(lines) == 2


def test_catalog_unknown_placeholder_fails(tmp_path):
    # REQ-1249: a placeholder naming no catalog field fails.
    with pytest.raises(YamaaError) as ei:
        _catalog_spec(
            tmp_path,
            "ID,CODE,N\nsysbp,SYSBP,1\n",
            {"USUBJID": "DM.USUBJID", "X": {"literal": "${NOPE}"}},
        )
    e = ei.value
    assert (e.phase, e.condition, e.requirement) == (
        "validation",
        "unknown_row_catalog_column",
        "REQ-1249",
    )


def test_catalog_duplicate_id_fails(tmp_path):
    # REQ-1249: id_column values must be unique identifiers.
    with pytest.raises(YamaaError) as ei:
        _catalog_spec(
            tmp_path,
            "ID,CODE,N\nsysbp,SYSBP,1\nsysbp,DIABP,2\n",
            {"USUBJID": "DM.USUBJID", "X": {"literal": "${CODE}"}},
        )
    e = ei.value
    assert (e.phase, e.condition) == ("validation", "invalid_row_catalog")


# -- mapping unmapped result (REQ-1110) --------------------------------------


def test_mapping_unmapped_result_answers(tmp_path):
    # REQ-1110: a present source with no dictionary entry returns the
    # unmapped result; a missing source still returns missing.
    spec = base_spec(
        derivation={
            "mapping": {
                "source": "DM.SEX",
                "dict": {"M": "Male", "F": "Female"},
                "missing": "Unknown",
                "unmapped": "Outside codelist",
            }
        }
    )
    inputs = {"dm.csv": "USUBJID,SEX\nS1,M\nS2,U\nS3,\n"}
    out = run(tmp_path, spec, inputs)
    lines = out.splitlines()
    assert lines[1].endswith(",Male")
    assert lines[2].endswith(",Outside codelist")
    assert lines[3].endswith(",Unknown")


# -- unresolvable_name diagnostic (REQ-0189, issue #780) ---------------------


def test_unqualified_input_field_suggests_qualified_spelling(tmp_path):
    # REQ-0189: an unqualified identifier naming a field of the row's
    # origin dataset fails unresolvable_name with the qualified
    # suggestion.
    spec = base_spec(derivation="VAL")
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, spec, {"dm.csv": DM})
    e = ei.value
    assert (e.phase, e.condition, e.requirement) == (
        "validation",
        "unresolvable_name",
        "REQ-0189",
    )
    assert e.context["suggestion"] == "DM.VAL"


def test_genuinely_unknown_name_stays_unknown_field(tmp_path):
    # REQ-0189: any other unavailable identifier fails unknown_field.
    spec = base_spec(derivation="NOPE")
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, spec, {"dm.csv": DM})
    e = ei.value
    assert (e.phase, e.condition) == ("validation", "unknown_field")


def test_case_when_reports_predicate_site(tmp_path):
    # REQ-0189: an unknown identifier in a case `when` reports at the
    # nested predicate site, keyed by identifier.
    spec = base_spec(
        derivation={"case": [{"when": "NOPE = 'x'", "then": {"literal": "y"}}]}
    )
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, spec, {"dm.csv": DM})
    e = ei.value
    assert e.condition == "unknown_field"
    assert e.requirement == "REQ-0189"
    assert e.context["identifier"] == "NOPE"
    assert e.spec_paths == ["columns.X.derivation.case[0].when"]
