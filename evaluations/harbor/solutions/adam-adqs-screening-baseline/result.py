# Reference solution for the yamaa benchmark adam-adqs-screening-baseline (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

INPUT = Path("/app/input")
OUTPUT = Path("/app/output")

KEYS = ["STUDYID", "USUBJID", "PARAMCD"]
COLUMNS = [
    "STUDYID", "USUBJID", "PARAMCD", "PARAM", "VISITNUM", "VISIT",
    "ADT", "TRTSDT", "AVAL", "ABLFL", "SCBLFL", "BASE", "CHG",
]

adsl = pl.read_csv(INPUT / "adsl.csv", infer_schema_length=0).select(
    "STUDYID",
    "USUBJID",
    pl.col("TRTSDT").str.strip_chars().replace("", None).str.to_date("%Y-%m-%d", strict=False),
)

qs = pl.read_csv(INPUT / "qs.csv", infer_schema_length=0)

adqs = (
    qs.select(
        "STUDYID",
        "USUBJID",
        pl.col("QSTESTCD").alias("PARAMCD"),
        pl.col("QSTEST").alias("PARAM"),
        pl.col("VISITNUM").cast(pl.Int64),
        "VISIT",
        # ISO 8601 date part of QSDTC; empty when the visit date is not collected.
        pl.col("QSDTC").str.strip_chars().replace("", None)
        .str.slice(0, 10).str.to_date("%Y-%m-%d", strict=False).alias("ADT"),
        pl.col("QSSTRESN").str.strip_chars().replace("", None)
        .cast(pl.Float64).alias("AVAL"),
    )
    .join(adsl, on=["STUDYID", "USUBJID"], how="left")
)

has_rating = pl.col("AVAL").is_not_null()
on_or_before_trt = (
    pl.col("ADT").is_not_null()
    & pl.col("TRTSDT").is_not_null()
    & (pl.col("ADT") <= pl.col("TRTSDT"))
)
c1d1_ok = (pl.col("VISIT") == "CYCLE 1 DAY 1") & has_rating & on_or_before_trt
scr_ok = (pl.col("VISIT") == "SCREENING") & has_rating

adqs = adqs.with_columns(
    c1d1_ok.any().over(KEYS).alias("_C1D1_BASE"),
)
adqs = adqs.with_columns(
    pl.when(c1d1_ok | (~pl.col("_C1D1_BASE") & scr_ok))
    .then(pl.lit("Y"))
    .otherwise(pl.lit(None, dtype=pl.String))
    .alias("ABLFL"),
)
adqs = adqs.with_columns(
    pl.when((pl.col("ABLFL") == "Y") & (pl.col("VISIT") == "SCREENING"))
    .then(pl.lit("Y"))
    .otherwise(pl.lit(None, dtype=pl.String))
    .alias("SCBLFL"),
    pl.col("AVAL").filter(pl.col("ABLFL") == "Y").first().over(KEYS).alias("BASE"),
)
adqs = adqs.with_columns(
    pl.when(
        pl.col("ADT").is_not_null()
        & pl.col("TRTSDT").is_not_null()
        & (pl.col("ADT") > pl.col("TRTSDT"))
        & pl.col("ABLFL").is_null()
    )
    .then(pl.col("AVAL") - pl.col("BASE"))
    .otherwise(None)
    .alias("CHG"),
)


def as_text(name: str) -> pl.Expr:
    """Render a numeric value without a trailing '.0' when it is whole."""
    col = pl.col(name)
    return (
        pl.when(col.is_null())
        .then(None)
        .when(col == col.round(0))
        .then(col.round(0).cast(pl.Int64).cast(pl.String))
        .otherwise(col.cast(pl.String))
        .alias(name)
    )


adqs = (
    adqs.sort(["STUDYID", "USUBJID", "PARAMCD", "VISITNUM"])
    .with_columns(as_text("AVAL"), as_text("BASE"), as_text("CHG"))
    .select(COLUMNS)
)

OUTPUT.mkdir(parents=True, exist_ok=True)
adqs.write_csv(OUTPUT / "adqs.csv", date_format="%Y-%m-%d", null_value="")
