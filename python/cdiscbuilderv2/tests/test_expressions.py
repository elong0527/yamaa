import pytest
import polars as pl
from cdiscbuilderv2.expressions import ExpressionParser


def test_expressions_mapping():
    parser = ExpressionParser()
    df = pl.DataFrame({"SEX": ["Male", "Female", "Unknown", None]})
    schema = {c: None for c in df.columns}

    spec = {
        "mapping": {
            "source": "SEX",
            "dict": {"Male": "M", "Female": "F"},
            "unmapped": "U",
            "missing": "U"
        }
    }
    expr = parser.parse(spec, schema)
    res = df.with_columns(expr.alias("SEX_MAPPED"))
    assert res["SEX_MAPPED"].to_list() == ["M", "F", "U", "U"]


def test_expressions_cut():
    parser = ExpressionParser()
    df = pl.DataFrame({"AGE": [10, 18, 45, 65, 80]})
    schema = {c: None for c in df.columns}

    spec = {
        "cut": {
            "source": "AGE",
            "breaks": [0, 18, 65, 120],
            "labels": ["<18", "18-64", ">=65"]
        }
    }
    expr = parser.parse(spec, schema)
    res = df.with_columns(expr.alias("AGE_CAT"))
    assert res["AGE_CAT"].to_list() == ["<18", "18-64", "18-64", ">=65", ">=65"]


def test_expressions_study_day():
    parser = ExpressionParser()
    df = pl.DataFrame({
        "AESTDTC": ["2026-07-20", "2026-07-16", "2026-07-10"],
        "RFSTDTC": ["2026-07-16", "2026-07-16", "2026-07-16"]
    })
    schema = {c: None for c in df.columns}

    spec = {
        "calculate_study_day": {
            "date": "AESTDTC",
            "reference_date": "RFSTDTC"
        }
    }
    expr = parser.parse(spec, schema)
    res = df.with_columns(expr.alias("AESTDY"))
    assert res["AESTDY"].to_list() == [5, 1, -6]


def test_expressions_string_operations():
    parser = ExpressionParser()
    df = pl.DataFrame({"TERM": ["Headache Severe", "Nausea", "fever"]})
    schema = {c: None for c in df.columns}

    spec_upper = {"str_upper": "TERM"}
    res_upper = df.with_columns(parser.parse(spec_upper, schema).alias("UPPER"))
    assert res_upper["UPPER"][0] == "HEADACHE SEVERE"

    spec_concat = {"str_concat": {"sources": ["TERM", " (Reported)"]}}
    res_concat = df.with_columns(parser.parse(spec_concat, schema).alias("CONCAT"))
    assert res_concat["CONCAT"][1] == "Nausea (Reported)"
