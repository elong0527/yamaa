# Reference solution for the yamaa benchmark adam-adlb-shift-criteria (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

raw = pl.read_csv("/app/input/adlb_raw.csv", infer_schema=False).with_columns(
    pl.col("ASEQ").cast(pl.Int64),
    pl.col("AVAL").cast(pl.Float64, strict=False),
    pl.col("ANRLO").cast(pl.Float64, strict=False),
    pl.col("ANRHI").cast(pl.Float64, strict=False),
)

# The record's own mark: LOW below the lower limit, HIGH above the upper
# limit, NORMAL between them limits included. No value when the result or
# either limit is missing.
based = raw.with_columns(
    ANRIND=pl.when(
        pl.col("AVAL").is_null()
        | pl.col("ANRLO").is_null()
        | pl.col("ANRHI").is_null()
    )
    .then(None)
    .when(pl.col("AVAL") < pl.col("ANRLO"))
    .then(pl.lit("LOW"))
    .when(pl.col("AVAL") > pl.col("ANRHI"))
    .then(pl.lit("HIGH"))
    .otherwise(pl.lit("NORMAL")),
)

# BASE repeats the flagged baseline record's value; BNRIND repeats its
# mark. Both have no value when no record carries the flag.
baselines = (
    based.filter(pl.col("ABLFL") == "Y")
    .select("STUDYID", "USUBJID", "PARAMCD", BASE="AVAL", BNRIND="ANRIND")
)
with_base = based.join(
    baselines, on=["STUDYID", "USUBJID", "PARAMCD"], how="left"
)

# SHIFT1 joins the baseline mark and the record's own mark; R2BASE is the
# record as a multiple of the baseline; CRIT1 assesses greater than three
# times the upper limit.
adlb = (
    with_base.with_columns(
        SHIFT1=pl.when(
            pl.col("BNRIND").is_null() | pl.col("ANRIND").is_null()
        )
        .then(None)
        .otherwise(pl.col("BNRIND") + pl.lit(" to ") + pl.col("ANRIND")),
        R2BASE=pl.when(
            pl.col("AVAL").is_null()
            | pl.col("BASE").is_null()
            | (pl.col("BASE") == 0)
        )
        .then(None)
        .otherwise(pl.col("AVAL") / pl.col("BASE")),
        CRITLIM=3 * pl.col("ANRHI"),
    )
    .with_columns(
        CRIT1=pl.when(
            pl.col("AVAL").is_not_null() & pl.col("CRITLIM").is_not_null()
        )
        .then(pl.lit("Result greater than 3 x ULN"))
        .otherwise(None),
        CRIT1FL=pl.when(
            pl.col("AVAL").is_null() | pl.col("CRITLIM").is_null()
        )
        .then(None)
        .when(pl.col("AVAL") > pl.col("CRITLIM"))
        .then(pl.lit("Y"))
        .otherwise(pl.lit("N")),
    )
    .select(
        "STUDYID",
        "USUBJID",
        "PARAMCD",
        "PARAM",
        "ASEQ",
        "AVISIT",
        "AVAL",
        "ANRLO",
        "ANRHI",
        "ANRIND",
        "ABLFL",
        "BASE",
        "BNRIND",
        "SHIFT1",
        "R2BASE",
        "CRIT1",
        "CRIT1FL",
    )
    .sort(["USUBJID", "ASEQ"])
)

Path("/app/output").mkdir(exist_ok=True)
adlb.write_csv("/app/output/adlb.csv")
