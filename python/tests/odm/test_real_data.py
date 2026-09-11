from __future__ import annotations

import os
from pathlib import Path

import polars as pl
import pytest
from yamaa.odm.normalize import normalize_odm
from yamaa.odm.schema import ODM_ITEM_SCHEMA

CART_T_SHA256 = "d865b44603df4fd03f0569ad3d88d18c1e180688025adc2e97de805477a689cf"
KN189_SHA256 = "117ea37baebfd640aa9a436528bb66336946e6db3213404769898f9b47955ec8"


def _fixture(variable: str) -> Path:
    value = os.environ.get(variable)
    if value is None:
        pytest.skip(f"set {variable} to run the pinned full-data test")
    return Path(value)


def test_full_cart_t_export(tmp_path: Path) -> None:
    source = _fixture("YAMAA_CART_T_ODM")
    output = tmp_path / "cart-t.parquet"

    result = normalize_odm(source, output, profile="cart-t-openclinica")
    data = pl.scan_parquet(output)
    summary = (
        data.select(
            pl.len().alias("rows"),
            (pl.col("Value") == "").sum().alias("empty_values"),
            pl.struct("StudyOID", "SubjectKey").n_unique().alias("subjects"),
            pl.col("SourceOrdinal").n_unique().alias("ordinals"),
        )
        .collect()
        .row(0, named=True)
    )

    assert result.row_count == 48_557
    assert result.source_sha256 == CART_T_SHA256
    assert summary == {
        "rows": 48_557,
        "empty_values": 1_345,
        "subjects": 810,
        "ordinals": 48_557,
    }
    assert pl.read_parquet_schema(output) == ODM_ITEM_SCHEMA


def test_full_kn189_export(tmp_path: Path) -> None:
    source = _fixture("YAMAA_KN189_ODM_ARCHIVE")
    output = tmp_path / "kn189.parquet"

    result = normalize_odm(source, output, profile="kn189")
    data = pl.scan_parquet(output)
    summary = (
        data.select(
            pl.len().alias("rows"),
            pl.struct("StudyOID", "SubjectKey").n_unique().alias("subjects"),
            pl.col("SourceOrdinal").n_unique().alias("ordinals"),
            pl.col("StudySubjectID").is_not_null().sum().alias("study_subject_ids"),
        )
        .collect()
        .row(0, named=True)
    )

    assert result.row_count == 908_490
    assert result.source_sha256 == KN189_SHA256
    assert summary == {
        "rows": 908_490,
        "subjects": 616,
        "ordinals": 908_490,
        "study_subject_ids": 0,
    }
    assert pl.read_parquet_schema(output) == ODM_ITEM_SCHEMA
    assert data.select("FormOID", "ItemGroupOID", "ItemOID", "ItemName", "Value").head(
        3
    ).collect().rows() == [
        (
            "FO.NCT02578680.BM",
            "IG.NCT02578680.BM",
            "IT.NCT02578680.BM.BMDTC",
            "BMDTC",
            "2016-03-11",
        ),
        (
            "FO.NCT02578680.BM",
            "IG.NCT02578680.BM",
            "IT.NCT02578680.BM.BMTEST",
            "BMTEST",
            "PD-L1 Tumor Proportion Score",
        ),
        (
            "FO.NCT02578680.BM",
            "IG.NCT02578680.BM",
            "IT.NCT02578680.BM.BMMETHOD",
            "BMMETHOD",
            "IHC 22C3 pharmDx",
        ),
    ]
