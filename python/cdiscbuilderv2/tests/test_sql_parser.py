import pytest
import polars as pl
from cdiscbuilderv2.sql_parser import SQLParser


def test_sql_parser_basic_equality():
    df = pl.DataFrame({
        "DOMAIN": ["DM", "AE", "LB"],
        "AGE": [25, 65, 40]
    })
    schema = {c: None for c in df.columns}

    expr_str = SQLParser.parse_to_expr("DOMAIN = 'DM'", schema)
    res_str = df.filter(expr_str)
    assert res_str.height == 1
    assert res_str["DOMAIN"][0] == "DM"

    expr_num = SQLParser.parse_to_expr("AGE = 65", schema)
    res_num = df.filter(expr_num)
    assert res_num.height == 1
    assert res_num["AGE"][0] == 65


def test_sql_parser_compound_and_or():
    df = pl.DataFrame({
        "ItemOID": ["IT.DM.SEX", "IT.DM.AGE", "IT.LB.CALCIUM", "IT.LB.CALCIUM"],
        "Value": ["M", "30", "9.5", None]
    })
    schema = {c: None for c in df.columns}

    # Test "ODM.ItemOID = 'IT.LB.CALCIUM' AND ODM.Value IS NOT NULL"
    expr = SQLParser.parse_to_expr("ODM.ItemOID = 'IT.LB.CALCIUM' AND ODM.Value IS NOT NULL", schema)
    res = df.filter(expr)
    assert res.height == 1
    assert res["ItemOID"][0] == "IT.LB.CALCIUM"
    assert res["Value"][0] == "9.5"


def test_sql_parser_in_and_comparison():
    df = pl.DataFrame({
        "AESEV": ["MILD", "MODERATE", "SEVERE"],
        "AESTDY": [1, 5, 20]
    })
    schema = {c: None for c in df.columns}

    expr_in = SQLParser.parse_to_expr("AESEV IN ('MODERATE', 'SEVERE')", schema)
    res_in = df.filter(expr_in)
    assert res_in.height == 2

    expr_comp = SQLParser.parse_to_expr("AESTDY >= 5", schema)
    res_comp = df.filter(expr_comp)
    assert res_comp.height == 2
