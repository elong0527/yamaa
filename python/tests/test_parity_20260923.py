"""Edge-case tests for behavior added during the 2026-09-23 parity push.

The benchmark corpus covers the golden paths; these pin the boundary
conditions the new rules text calls out explicitly (REQ-0142 grouped-row
aggregate key rejection, REQ-1185 intermediate derivations, REQ-1245
uniqueness verification, REQ-1243/1244 str_contains, REQ-1107 to_date ISO
text, qualified key_base scalar resolution, inline lookup cache identity).
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


DM2 = "USUBJID,NAME\nS1,abc\nS2,xyz\n"


def adsl_spec(**kw):
    spec = {
        "schema_version": "1.0",
        "domain": "ADSL",
        "keys": ["USUBJID"],
        "input": {"DM": {"path": "input/dm.csv"}},
        "output": {"path": "adsl.csv", "columns": ["USUBJID", "FLAG"]},
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "DM.USUBJID"},
            {"name": "FLAG", "type": "str", "derivation": kw["derivation"]},
        ],
    }
    spec.update(kw.get("root", {}))
    return spec


# ------------------------------------------------------- REQ-1244 predicate


def test_str_contains_predicate_matches(tmp_path):
    spec = adsl_spec(
        derivation={
            "case": [
                {"when": "str_contains(DM.NAME, 'b.')", "then": {"literal": "Y"}},
                {"otherwise": {"literal": "N"}},
            ]
        },
        root={"filter": "str_contains(DM.NAME, '^[a-z]+$')"},
    )
    out = run(tmp_path, spec, {"dm.csv": DM2})
    lines = out.splitlines()
    assert lines[1] == "S1,Y"
    assert lines[2] == "S2,N"


def test_str_contains_predicate_case_insensitive_name(tmp_path):
    spec = adsl_spec(
        derivation={
            "case": [
                {"when": "STR_Contains(DM.NAME, 'b')", "then": {"literal": "Y"}},
                {"otherwise": {"literal": "N"}},
            ]
        }
    )
    out = run(tmp_path, spec, {"dm.csv": DM2})
    assert out.splitlines()[1] == "S1,Y"


def test_str_contains_predicate_missing_is_unknown(tmp_path):
    # REQ-1244: a missing source is UNKNOWN, so the filter drops the row
    # (and NOT UNKNOWN stays UNKNOWN).
    dm = "USUBJID,NAME\nS1,abc\nS2,\n"
    spec = {
        "schema_version": "1.0",
        "domain": "ADSL",
        "keys": ["USUBJID"],
        "input": {"DM": {"path": "input/dm.csv"}},
        "filter": "str_contains(DM.NAME, 'b')",
        "output": {"path": "adsl.csv", "columns": ["USUBJID", "FLAG"]},
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "DM.USUBJID"},
            {
                "name": "FLAG",
                "type": "str",
                "derivation": {
                    "case": [
                        {
                            "when": "NOT str_contains(DM.NAME, 'b')",
                            "then": {"literal": "Y"},
                        },
                        {"otherwise": {"literal": "N"}},
                    ]
                },
            },
        ],
    }
    out = run(tmp_path, spec, {"dm.csv": dm})
    # S1 matches (FLAG N: NOT TRUE is FALSE); S2 is missing -> UNKNOWN,
    # dropped by the filter.
    assert out.splitlines() == ["USUBJID,FLAG", "S1,N"]


def test_str_contains_predicate_rejects_other_calls(tmp_path):
    e = expect_error(
        tmp_path,
        adsl_spec(
            derivation={
                "case": [
                    {"when": "matches(DM.NAME, 'b')", "then": {"literal": "Y"}},
                    {"otherwise": {"literal": "N"}},
                ]
            }
        ),
        {"dm.csv": DM2},
    )
    assert (e.phase, e.condition, e.requirement) == (
        "validation",
        "invalid_predicate",
        "REQ-1244",
    )


def test_str_contains_predicate_rejects_bad_pattern(tmp_path):
    e = expect_error(
        tmp_path,
        adsl_spec(
            derivation={
                "case": [
                    {
                        "when": "str_contains(DM.NAME, 'SEVERE[')",
                        "then": {"literal": "Y"},
                    },
                    {"otherwise": {"literal": "N"}},
                ]
            }
        ),
        {"dm.csv": DM2},
    )
    assert (e.phase, e.condition, e.requirement) == (
        "validation",
        "invalid_predicate",
        "REQ-1244",
    )


def test_str_contains_predicate_rejects_nonliteral_pattern(tmp_path):
    e = expect_error(
        tmp_path,
        adsl_spec(
            derivation={
                "case": [
                    {
                        "when": "str_contains(DM.NAME, DM.NAME)",
                        "then": {"literal": "Y"},
                    },
                    {"otherwise": {"literal": "N"}},
                ]
            }
        ),
        {"dm.csv": DM2},
    )
    assert (e.phase, e.condition, e.requirement) == (
        "validation",
        "invalid_predicate",
        "REQ-1244",
    )


# ------------------------------------------------------ REQ-1243 expression


def test_str_contains_expression_missing_handler(tmp_path):
    # REQ-1243: the missing handler answers a missing source. (A
    # non-missing source yields bool, which R011 never converts.)
    dm = "USUBJID,NAME\nS1,\n"
    spec = adsl_spec(
        derivation={
            "str_contains": {"source": "DM.NAME", "pattern": "a", "missing": "N"}
        }
    )
    out = run(tmp_path, spec, {"dm.csv": dm})
    assert out.splitlines()[1] == "S1,N"


def test_str_contains_expression_bad_pattern_invalid_regex(tmp_path):
    e = expect_error(
        tmp_path,
        adsl_spec(derivation={"str_contains": {"source": "DM.NAME", "pattern": "a["}}),
        {"dm.csv": DM2},
    )
    assert (e.phase, e.condition) == ("validation", "invalid_regex")


# ------------------------------------------------------------- REQ-1107


def test_to_date_iso_text(tmp_path):
    # REQ-1107: to_date accepts a complete ISO date text as well as a
    # datetime.
    dm = "USUBJID,DTC\nS1,2024-03-09\nS2,\n"
    spec = adsl_spec(derivation={"to_date": {"source": "DM.DTC"}})
    spec["columns"][1]["type"] = "date"
    out = run(tmp_path, spec, {"dm.csv": dm})
    assert out.splitlines() == ["USUBJID,FLAG", "S1,2024-03-09", "S2,"]


def test_to_date_partial_text_invalid(tmp_path):
    dm = "USUBJID,DTC\nS1,2024-03\n"
    spec = adsl_spec(derivation={"to_date": {"source": "DM.DTC"}})
    spec["columns"][1]["type"] = "date"
    e = expect_error(tmp_path, spec, {"dm.csv": dm})
    assert (e.phase, e.condition, e.requirement) == (
        "derivation",
        "invalid_date_text",
        "REQ-1107",
    )


# ------------------------------------------------------------- REQ-0142


def test_grouped_row_aggregate_with_key_rejected(tmp_path):
    # REQ-0142: a grouped row-template aggregate reads its own group and
    # declares no key pairs.
    ae = "USUBJID,AEDECOD\nS1,HEADACHE\nS1,NAUSEA\n"
    spec = {
        "schema_version": "1.0",
        "domain": "ADAE",
        "keys": ["USUBJID"],
        "input": {
            "DM": {"path": "input/dm.csv"},
            "AE": {"path": "input/ae.csv"},
        },
        "output": {"path": "adae.csv", "columns": ["USUBJID", "N"]},
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "DM.USUBJID"},
            {"name": "N", "type": "int", "derivation": "N"},
        ],
        "rows": [
            {
                "id": "ae",
                "dataset": "AE",
                "group_by": ["AE.USUBJID"],
                "derivations": {
                    "N": {
                        "aggregate": {
                            "expr": "COUNT(AE.AEDECOD)",
                            "key": ["USUBJID"],
                            "key_base": ["AE.USUBJID"],
                        }
                    },
                },
            }
        ],
    }
    e = expect_error(tmp_path, spec, {"dm.csv": "USUBJID\nS1\n", "ae.csv": ae})
    assert (e.phase, e.condition, e.requirement) == (
        "validation",
        "invalid_aggregate_context",
        "REQ-0142",
    )
    assert e.spec_paths == [
        "rows.ae.derivations.N.aggregate.key",
        "rows.ae.derivations.N.aggregate.key_base",
    ]


# ------------------------------------------------------------- REQ-1185


def test_intermediate_derivation_in_filter(tmp_path):
    # REQ-1185: derivations compute once per donor record and derived
    # names behave like stored fields for the filter.
    dm = "USUBJID\nS1\nS2\n"
    ds = "USUBJID,DSCAT,DSDECOD\nS1,DISPOSITION EVENT,COMPLETED\nS2,OTHER,COMPLETED\n"
    spec = {
        "schema_version": "1.0",
        "domain": "ADSL",
        "keys": ["USUBJID"],
        "base": "DM",
        "input": {
            "DM": {"path": "input/dm.csv"},
            "DS": {"path": "input/ds.csv"},
        },
        "intermediates": [
            {
                "id": "EOT",
                "dataset": "DS",
                "derivations": {
                    "CAT_UP": {"str_upper": {"source": "DS.DSCAT"}},
                },
                "filter": "DS.CAT_UP = 'DISPOSITION EVENT'",
                "order_by": ["DS.DSDECOD"],
                "keep": "first",
                "columns": ["DSDECOD", "CAT_UP"],
            }
        ],
        "output": {"path": "adsl.csv", "columns": ["USUBJID", "DSDECOD", "CAT_UP"]},
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "DM.USUBJID"},
            {"name": "DSDECOD", "type": "str", "derivation": "EOT.DSDECOD"},
            {"name": "CAT_UP", "type": "str", "derivation": "EOT.CAT_UP"},
        ],
    }
    out = run(tmp_path, spec, {"dm.csv": dm, "ds.csv": ds})
    assert out.splitlines() == [
        "USUBJID,DSDECOD,CAT_UP",
        "S1,COMPLETED,DISPOSITION EVENT",
        "S2,,",
    ]


def test_intermediate_derivation_driver_reference_rejected(tmp_path):
    # REQ-1185: a derivation may not read driver fields.
    spec = {
        "schema_version": "1.0",
        "domain": "ADSL",
        "keys": ["USUBJID"],
        "base": "DM",
        "input": {
            "DM": {"path": "input/dm.csv"},
            "DS": {"path": "input/ds.csv"},
        },
        "intermediates": [
            {
                "id": "EOT",
                "dataset": "DS",
                "derivations": {"X": {"source": "DM.USUBJID"}},
            }
        ],
        "output": {"path": "adsl.csv", "columns": ["USUBJID"]},
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "DM.USUBJID"},
        ],
    }
    e = expect_error(
        tmp_path, spec, {"dm.csv": "USUBJID\nS1\n", "ds.csv": "USUBJID\nS1\n"}
    )
    assert (e.phase, e.condition, e.requirement) == (
        "validation",
        "unknown_field",
        "REQ-1185",
    )


def test_intermediate_derivation_shadow_rejected(tmp_path):
    # REQ-1185: a derived name may not shadow a stored column.
    spec = {
        "schema_version": "1.0",
        "domain": "ADSL",
        "keys": ["USUBJID"],
        "base": "DM",
        "input": {
            "DM": {"path": "input/dm.csv"},
            "DS": {"path": "input/ds.csv"},
        },
        "intermediates": [
            {
                "id": "EOT",
                "dataset": "DS",
                "derivations": {"DSCAT": {"source": "DS.DSCAT"}},
            }
        ],
        "output": {"path": "adsl.csv", "columns": ["USUBJID"]},
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "DM.USUBJID"},
        ],
    }
    e = expect_error(
        tmp_path, spec, {"dm.csv": "USUBJID\nS1\n", "ds.csv": "USUBJID,DSCAT\nS1,X\n"}
    )
    assert (e.phase, e.condition, e.requirement) == (
        "validation",
        "duplicate_derivation",
        "REQ-1185",
    )


# ------------------------------------------------------------- REQ-1245


def test_intermediate_verification_unique_duplicate(tmp_path):
    # REQ-1245: a repeated unique combination fails the run before any
    # row is built.
    dm = "USUBJID\nS1\n"
    ds = "USUBJID,DSCAT\nS1,DISPOSITION EVENT\nS1,DISPOSITION EVENT\n"
    spec = {
        "schema_version": "1.0",
        "domain": "ADSL",
        "keys": ["USUBJID"],
        "base": "DM",
        "input": {
            "DM": {"path": "input/dm.csv"},
            "DS": {"path": "input/ds.csv"},
        },
        "intermediates": [
            {
                "id": "EOT",
                "dataset": "DS",
                "filter": "DS.DSCAT = 'DISPOSITION EVENT'",
                "verification": {"unique": ["USUBJID"]},
            }
        ],
        "output": {"path": "adsl.csv", "columns": ["USUBJID", "DSCAT"]},
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "DM.USUBJID"},
            {"name": "DSCAT", "type": "str", "derivation": "EOT.DSCAT"},
        ],
    }
    e = expect_error(tmp_path, spec, {"dm.csv": dm, "ds.csv": ds})
    assert (e.phase, e.condition, e.requirement) == (
        "verification",
        "duplicate_intermediate_records",
        "REQ-1245",
    )


def test_intermediate_verification_unique_ok(tmp_path):
    dm = "USUBJID\nS1\nS2\n"
    ds = "USUBJID,DSCAT\nS1,DISPOSITION EVENT\nS2,DISPOSITION EVENT\n"
    spec = {
        "schema_version": "1.0",
        "domain": "ADSL",
        "keys": ["USUBJID"],
        "base": "DM",
        "input": {
            "DM": {"path": "input/dm.csv"},
            "DS": {"path": "input/ds.csv"},
        },
        "intermediates": [
            {
                "id": "EOT",
                "dataset": "DS",
                "filter": "DS.DSCAT = 'DISPOSITION EVENT'",
                "verification": {"unique": ["USUBJID"]},
                "key": ["USUBJID"],
                "columns": ["DSCAT"],
            }
        ],
        "output": {"path": "adsl.csv", "columns": ["USUBJID", "DSCAT"]},
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "DM.USUBJID"},
            {"name": "DSCAT", "type": "str", "derivation": "EOT.DSCAT"},
        ],
    }
    out = run(tmp_path, spec, {"dm.csv": dm, "ds.csv": ds})
    assert out.splitlines() == [
        "USUBJID,DSCAT",
        "S1,DISPOSITION EVENT",
        "S2,DISPOSITION EVENT",
    ]


# --------------------------------- qualified key_base scalar resolution


def test_column_aggregate_qualified_key_base(tmp_path):
    # A column-level aggregate's qualified key_base resolves the row's
    # carried group values, not a scan of all origin records.
    dm = "USUBJID\nS1\nS2\n"
    ae = "USUBJID,AEDECOD\nS1,HEADACHE\nS1,NAUSEA\nS2,HEADACHE\n"
    spec = {
        "schema_version": "1.0",
        "domain": "ADAE",
        "keys": ["USUBJID"],
        "input": {
            "DM": {"path": "input/dm.csv"},
            "AE": {"path": "input/ae.csv"},
        },
        "output": {"path": "adae.csv", "columns": ["USUBJID", "N"]},
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "DM.USUBJID"},
            {
                "name": "N",
                "type": "int",
                "derivation": {
                    "aggregate": {
                        "expr": "COUNT(AE.AEDECOD)",
                        "key": ["USUBJID"],
                        "key_base": ["AE.USUBJID"],
                    }
                },
            },
        ],
        "rows": [
            {
                "id": "dm",
                "dataset": "DM",
                "group_by": ["DM.USUBJID"],
            }
        ],
    }
    out = run(tmp_path, spec, {"dm.csv": dm, "ae.csv": ae})
    assert out.splitlines() == ["USUBJID,N", "S1,2", "S2,1"]


# --------------------------------------- inline lookup cache identity


def test_inline_lookup_cache_identity(tmp_path):
    # Two inline lookups over the same dataset and value with different
    # filters are different declarations and must not share results.
    dm = "USUBJID\nS1\n"
    cm = "USUBJID,CMDECOD,ATC\nS1,ASPIRIN,A\nS1,ASPIRIN,B\n"
    spec = {
        "schema_version": "1.0",
        "domain": "ADCM",
        "keys": ["USUBJID"],
        "base": "DM",
        "input": {
            "DM": {"path": "input/dm.csv"},
            "CM": {"path": "input/cm.csv"},
        },
        "output": {"path": "adcm.csv", "columns": ["USUBJID", "ATC_A", "ATC_B"]},
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "DM.USUBJID"},
            {
                "name": "ATC_A",
                "type": "str",
                "derivation": {
                    "lookup": {
                        "dataset": "CM",
                        "value": "ATC",
                        "key": ["USUBJID"],
                        "filter": "CM.ATC = 'A'",
                        "order_by": ["CM.ATC"],
                        "keep": "first",
                    }
                },
            },
            {
                "name": "ATC_B",
                "type": "str",
                "derivation": {
                    "lookup": {
                        "dataset": "CM",
                        "value": "ATC",
                        "key": ["USUBJID"],
                        "filter": "CM.ATC = 'B'",
                        "order_by": ["CM.ATC"],
                        "keep": "first",
                    }
                },
            },
        ],
    }
    out = run(tmp_path, spec, {"dm.csv": dm, "cm.csv": cm})
    assert out.splitlines() == ["USUBJID,ATC_A,ATC_B", "S1,A,B"]
