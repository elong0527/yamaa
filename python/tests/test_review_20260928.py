"""Boundary tests for the review fixes of 2026-09-28 (issues #1469-#1480).

The benchmark corpus pins the headline conditions; these pin the edges it
leaves open: selection and multiple matches on the implicit join
(REQ-0111/0119/0127), the ungrouped row filter (REQ-0036/0068), grouped
row counts (REQ-0386/0387/1154), reads of another intermediate from a
derivation (REQ-1263), container-supplied field types (REQ-0517/1032/1040),
the mapping handlers (REQ-1110), and the retired vocabulary.
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


DM = "USUBJID\nS1\nS2\n"
EX = "USUBJID,EXSEQ,EXTRT\nS1,1,A\nS1,2,B\nS2,1,C\n"


def join_spec(derivation, ex_types=None, rows=None):
    spec = {
        "schema_version": "1.0",
        "domain": "ADSL",
        "keys": ["USUBJID"],
        "base": "DM",
        "input": {
            "DM": {"path": "input/dm.csv"},
            "EX": {"path": "input/ex.csv", "types": ex_types or {"EXSEQ": "int"}},
        },
        "output": {"path": "adsl.csv", "columns": ["USUBJID", "X"]},
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "DM.USUBJID"},
            {"name": "X", "type": "str", "derivation": derivation},
        ],
    }
    if rows is not None:
        spec["rows"] = rows
    return spec


# -- the implicit join (REQ-0111/0119/0127) ---------------------------------


def test_source_selection_chooses_among_joined_records(tmp_path):
    # REQ-0111: a structured source keeps order_by/keep on the implicit
    # join and chooses among the survivors as a named lookup would.
    spec = join_spec(
        {"source": {"variable": "EX.EXTRT", "order_by": ["EX.EXSEQ"], "keep": "last"}}
    )
    out = run(tmp_path, spec, {"dm.csv": DM, "ex.csv": EX})
    assert out.splitlines() == ["USUBJID,X", "S1,B", "S2,C"]


def test_source_order_by_without_keep_is_unpaired(tmp_path):
    # REQ-0119: order_by and keep are declared together, before any data.
    spec = join_spec({"source": {"variable": "EX.EXTRT", "order_by": ["EX.EXSEQ"]}})
    e = expect_error(tmp_path, spec, {"dm.csv": DM, "ex.csv": EX})
    assert pinned(e) == ("validation", "unpaired_fields", "REQ-0119")
    assert e.spec_paths == ["columns.X.derivation.source"]


def test_joined_duplicates_fail_even_when_values_agree(tmp_path):
    # REQ-0127 counts surviving records, not distinct values: REQ-0075's
    # agreement rule belongs to the records a key combination came from.
    ex = "USUBJID,EXSEQ,EXTRT\nS1,1,A\nS1,2,A\nS2,1,C\n"
    e = expect_error(tmp_path, join_spec("EX.EXTRT"), {"dm.csv": DM, "ex.csv": ex})
    assert pinned(e) == ("join", "multiple_matches", "REQ-0127")
    assert e.spec_paths == ["columns.X.derivation.source"]
    assert e.context["match_count"] == 2
    assert e.context["keys"] == [{"USUBJID": "S1"}]


def test_row_phase_joined_duplicates_fail_at_join(tmp_path):
    # REQ-0156/REQ-0127: during row construction the join still binds one
    # value per row, and a multiple match is a join-phase condition.
    rows = [{"id": "dm", "dataset": "DM", "derivations": {"X": "EX.EXTRT"}}]
    spec = join_spec(None, rows=rows)
    del spec["columns"][1]["derivation"]
    e = expect_error(tmp_path, spec, {"dm.csv": DM, "ex.csv": EX})
    assert pinned(e) == ("join", "multiple_matches", "REQ-0127")


def test_implicit_join_index_keeps_comparison_semantics(tmp_path):
    # Issue #1485: the join reads through an equality index, which must
    # equal the REQ-0005 comparison: int and float keys match by value, and
    # a missing key value matches nothing.
    spec = {
        "schema_version": "1.0",
        "domain": "ADLB",
        "keys": ["USUBJID", "SEQ"],
        "base": "A",
        "input": {
            "A": {"path": "input/a.csv", "types": {"SEQ": "int"}},
            "B": {"path": "input/b.csv", "types": {"SEQ": "float"}},
        },
        "output": {"path": "adlb.csv", "columns": ["USUBJID", "SEQ", "X"]},
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "A.USUBJID"},
            {"name": "SEQ", "type": "int", "derivation": "A.SEQ"},
            {"name": "X", "type": "str", "derivation": "B.X"},
        ],
    }
    inputs = {
        "a.csv": "USUBJID,SEQ\nS1,1\nS1,2\n",
        "b.csv": "USUBJID,SEQ,X\nS1,1.0,one\nS1,,none\n",
    }
    out = run(tmp_path, spec, inputs)
    assert out.splitlines() == ["USUBJID,SEQ,X", "S1,1,one", "S1,2,"]


# -- ungrouped row filter (REQ-0036/0068) -----------------------------------


def filter_spec(filt, derivations):
    return {
        "schema_version": "1.0",
        "domain": "ADSL",
        "keys": ["USUBJID"],
        "input": {"DM": {"path": "input/dm.csv", "types": {"AGE": "int"}}},
        "output": {"path": "adsl.csv", "columns": ["USUBJID", "AGE", "AGEGR"]},
        "columns": [
            {"name": "USUBJID", "type": "str"},
            {"name": "AGE", "type": "int"},
            {"name": "AGEGR", "type": "str"},
        ],
        "rows": [
            {"id": "dm", "dataset": "DM", "filter": filt, "derivations": derivations}
        ],
    }


FILTER_DERIVS = {
    "USUBJID": "DM.USUBJID",
    "AGE": "DM.AGE",
    "AGEGR": {"mapping": {"source": "DM.GRP", "dict": {"a": "ADULT"}}},
}


def test_row_filter_reads_derived_column(tmp_path):
    # REQ-0036: the filter reads the candidate's derived columns. The
    # discarded record never derives AGEGR, whose mapping would fail on it.
    spec = filter_spec("AGE >= 18", FILTER_DERIVS)
    out = run(tmp_path, spec, {"dm.csv": "USUBJID,AGE,GRP\nS1,40,a\nS2,10,unlisted\n"})
    assert out.splitlines() == ["USUBJID,AGE,AGEGR", "S1,40,ADULT"]


def test_row_filter_naming_underived_column_fails(tmp_path):
    # REQ-0068: a bare filter name must be a column the template derives.
    spec = filter_spec("GRP = 'a'", FILTER_DERIVS)
    e = expect_error(tmp_path, spec, {"dm.csv": "USUBJID,AGE,GRP\nS1,40,a\n"})
    assert pinned(e) == ("validation", "unknown_field", "REQ-0068")
    assert e.spec_paths == ["rows.dm.filter"]


# -- grouped row counts (REQ-0386/0387/1154) --------------------------------


def count_spec(**row_count):
    return {
        "schema_version": "1.0",
        "domain": "ADLB",
        "keys": ["USUBJID", "SEQ"],
        "input": {"LB": {"path": "input/lb.csv", "types": {"SEQ": "int"}}},
        "output": {"path": "adlb.csv", "columns": ["USUBJID", "SEQ", "BL"]},
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "LB.USUBJID"},
            {"name": "SEQ", "type": "int", "derivation": "LB.SEQ"},
            {"name": "BL", "type": "str", "derivation": "LB.BL"},
        ],
        "verifications": [
            {"row_count": dict({"id": "one-bl", "group_by": ["USUBJID"]}, **row_count)}
        ],
    }


LB = "USUBJID,SEQ,BL\nS1,1,Y\nS1,2,\nS2,1,\n"


def test_row_count_group_without_filtered_rows_fails_min(tmp_path):
    # REQ-0386/0387: groups partition the artifact, so S2 -- holding no
    # flagged row -- is a group of count zero, not a missing group.
    spec = count_spec(filter="BL = 'Y'", min=1, max=1)
    e = expect_error(tmp_path, spec, {"lb.csv": LB})
    assert pinned(e) == ("verification", "row_count_failed", "REQ-0385")
    assert e.spec_paths == ["verifications[0].row_count"]


def test_row_count_when_exempts_unbound_groups(tmp_path):
    # REQ-1154: a group no row of which satisfies `when` is exempt.
    spec = count_spec(filter="BL = 'Y'", when="SEQ = 2", min=1, max=1)
    out = run(tmp_path, spec, {"lb.csv": LB})
    assert out.splitlines()[0] == "USUBJID,SEQ,BL"


# -- reads of another intermediate (REQ-1263) -------------------------------


def chain_spec(first, second_derivations):
    return {
        "schema_version": "1.0",
        "domain": "ADSL",
        "keys": ["USUBJID"],
        "base": "DM",
        "input": {
            "DM": {"path": "input/dm.csv"},
            "EX": {"path": "input/ex.csv", "types": {"EXSEQ": "int"}},
        },
        "intermediates": [
            first,
            {
                "id": "LAST",
                "dataset": "EX",
                "derivations": second_derivations,
                "order_by": ["EX.EXSEQ"],
                "keep": "last",
            },
        ],
        "output": {"path": "adsl.csv", "columns": ["USUBJID", "X"]},
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "DM.USUBJID"},
            {"name": "X", "type": "str", "derivation": "LAST.SUBJ"},
        ],
    }


SUBJECT = {"id": "SUBJ", "dataset": "DM", "key": ["USUBJID"], "columns": ["USUBJID"]}


def test_derivation_reads_another_intermediate(tmp_path):
    # REQ-1263: the read matches the other intermediate from the donor
    # record being augmented.
    spec = chain_spec(SUBJECT, {"SUBJ": "SUBJ.USUBJID"})
    out = run(tmp_path, spec, {"dm.csv": DM, "ex.csv": EX})
    assert out.splitlines() == ["USUBJID,X", "S1,S1", "S2,S2"]


def test_derivation_read_outside_columns_fails(tmp_path):
    # REQ-1263/REQ-0125: the read column must be one of `columns`.
    spec = chain_spec(dict(SUBJECT, columns=["USUBJID"]), {"SUBJ": "SUBJ.NOPE"})
    e = expect_error(tmp_path, spec, {"dm.csv": DM, "ex.csv": EX})
    assert pinned(e) == ("validation", "unknown_field", "REQ-1263")
    assert e.spec_paths == ["intermediates[1].derivations.SUBJ"]


def test_window_reading_another_intermediate_fails(tmp_path):
    # REQ-1263: a window's fields read only the donor records.
    window = {
        "row_number": {
            "window": {"group_by": ["SUBJ.USUBJID"], "order_by": ["EX.EXSEQ"]}
        }
    }
    spec = chain_spec(SUBJECT, {"SUBJ": window})
    e = expect_error(tmp_path, spec, {"dm.csv": DM, "ex.csv": EX})
    assert pinned(e) == ("validation", "unknown_field", "REQ-1185")


def test_intermediates_reading_each_other_fail_as_cycle(tmp_path):
    # REQ-1263: derivations that read each other are a dependency cycle.
    first = dict(SUBJECT, derivations={"BACK": "LAST.EXTRT"})
    spec = chain_spec(first, {"SUBJ": "SUBJ.USUBJID"})
    e = expect_error(tmp_path, spec, {"dm.csv": DM, "ex.csv": EX})
    assert pinned(e) == ("validation", "dependency_cycle", "REQ-1263")


# -- container-supplied field types (REQ-0517/1032/1040) --------------------


def test_undeclared_csv_field_is_str(tmp_path):
    # REQ-0517: a delimited file supplies no types, so a numeric-looking
    # undeclared field is `str` and cannot feed `cut`.
    spec = join_spec(
        {"cut": {"source": "DM.AGE", "breaks": [18], "labels": ["<18", ">=18"]}}
    )
    e = expect_error(tmp_path, spec, {"dm.csv": "USUBJID,AGE\nS1,40\n", "ex.csv": EX})
    assert pinned(e) == ("validation", "incompatible_input_type", "REQ-0306")
    assert e.context["actual"] == "str"


def test_parquet_field_outside_the_mapping_fails(tmp_path):
    # REQ-1032/REQ-1040: an INT32 field has no column type.
    import pyarrow as pa
    import pyarrow.parquet as pq

    os.makedirs(os.path.join(str(tmp_path), "input"), exist_ok=True)
    table = pa.table(
        {"USUBJID": pa.array(["S1"]), "AGE": pa.array([40], type=pa.int32())}
    )
    pq.write_table(table, os.path.join(str(tmp_path), "input", "dm.parquet"))
    spec = join_spec("DM.AGE")
    spec["input"] = {"DM": {"path": "input/dm.parquet"}}
    e = expect_error(tmp_path, spec, {})
    assert pinned(e) == ("ingest", "source_field_type_unsupported", "REQ-1040")
    assert e.context["field"] == "AGE"


# -- mapping handlers (REQ-1110) --------------------------------------------


def test_mapping_missing_source_without_handler_fails(tmp_path):
    # REQ-1110/REQ-0344: without `missing`, a missing source fails.
    spec = join_spec({"mapping": {"source": "DM.SEX", "dict": {"M": "Male"}}})
    e = expect_error(tmp_path, spec, {"dm.csv": "USUBJID,SEX\nS1,\n", "ex.csv": EX})
    assert pinned(e) == ("mapping", "missing_input", "REQ-0334")
    assert e.spec_paths == ["columns.X.derivation.mapping"]


# -- retired vocabulary ------------------------------------------------------


@pytest.mark.parametrize(
    "derivation",
    [
        {"str_upper": {"source": "DM.USUBJID"}},
        {"lookup": {"dataset": "EX", "value": "EXTRT", "key": ["USUBJID"]}},
    ],
    ids=["str_upper", "inline_lookup"],
)
def test_retired_expression_is_unknown(tmp_path, derivation):
    # #1471/#1480: the retired keywords are not registered expressions.
    e = expect_error(tmp_path, join_spec(derivation), {"dm.csv": DM, "ex.csv": EX})
    assert (e.phase, e.condition) == ("validation", "unknown_field")


def test_handled_wrapper_admits_only_unconvertible(tmp_path):
    # REQ-0358/REQ-0362: the result wrapper registers `unconvertible` only.
    derivation = {"value": {"source": "DM.USUBJID"}, "missing": "X"}
    e = expect_error(tmp_path, join_spec(derivation), {"dm.csv": DM, "ex.csv": EX})
    assert pinned(e) == ("validation", "invalid_field_type", "REQ-0286")


def test_intermediate_key_base_is_rejected(tmp_path):
    # #1480/REQ-0285: key_base is not an intermediate_class field.
    spec = join_spec("EXF.EXTRT")
    spec["intermediates"] = [
        {
            "id": "EXF",
            "dataset": "EX",
            "key": ["USUBJID"],
            "key_base": ["DM.USUBJID"],
            "order_by": ["EX.EXSEQ"],
            "keep": "first",
        }
    ]
    e = expect_error(tmp_path, spec, {"dm.csv": DM, "ex.csv": EX})
    assert pinned(e) == ("validation", "unknown_field", "REQ-0285")
    assert e.spec_paths == ["intermediates[0].key_base"]


def test_no_match_literal_is_not_rename_only(tmp_path):
    # REQ-1248: only `no_match: null` adds nothing to an alias; a literal
    # answers absence differently from the implicit join.
    spec = join_spec("EXF.EXTRT")
    spec["intermediates"] = [{"id": "EXF", "dataset": "EX", "no_match": "NONE"}]
    ex = "USUBJID,EXSEQ,EXTRT\nS1,1,A\n"
    out = run(tmp_path, spec, {"dm.csv": DM, "ex.csv": ex})
    assert out.splitlines() == ["USUBJID,X", "S1,A", "S2,NONE"]


def test_absent_does_not_answer_a_join_that_reaches_no_record(tmp_path):
    # REQ-0345: `absent` answers a variable absent from context. S3 has no
    # EX record, but EX.EXTRT still exists, so the read is missing exactly
    # as the main engine answers it (REQ-0111/REQ-0355).
    dm = "USUBJID\nS1\nS3\n"
    out = run(
        tmp_path,
        join_spec(
            {
                "source": {
                    "variable": "EX.EXTRT",
                    "order_by": ["EX.EXSEQ"],
                    "keep": "first",
                    "absent": "NONE",
                }
            }
        ),
        {"dm.csv": dm, "ex.csv": EX},
    )
    assert out.splitlines() == ["USUBJID,X", "S1,A", "S3,"]
