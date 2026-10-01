# Reference solution for the yamaa benchmark adam-adrs-composite-response (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

adsl = pl.read_csv("/app/input/adsl.csv", infer_schema=False)
raw = pl.read_csv("/app/input/adrs_raw.csv", infer_schema=False).with_columns(
    pl.col("PCHG").cast(pl.Float64)
)

base = raw.join(adsl.select("USUBJID", "SAEFL", "DCSREAS"), on="USUBJID", how="left")

# The checks apply in a fixed order: a safety or discontinuation rule
# first, then a missing component, then the 75% reduction threshold.
safety = (pl.col("SAEFL") == "Y") | (
    pl.col("DCSREAS").is_not_null() & (pl.col("DCSREAS") != "")
)
avalc = (
    pl.when(safety)
    .then(pl.lit("NON-RESPONDER"))
    .when(pl.col("PCHG").is_null())
    .then(pl.lit("NOT EVALUABLE"))
    .when(pl.col("PCHG") <= -75)
    .then(pl.lit("RESPONDER"))
    .otherwise(pl.lit("NON-RESPONDER"))
)
arsn = (
    pl.when(safety)
    .then(pl.lit("SAFETY OR DISCONTINUATION RULE"))
    .when(pl.col("PCHG").is_null())
    .then(pl.lit("COMPONENT MISSING"))
    .when(pl.col("PCHG") <= -75)
    .then(pl.lit("THRESHOLD MET"))
    .otherwise(pl.lit("THRESHOLD NOT MET"))
)

adrs = (
    base.with_columns(
        PARAMCD=pl.lit("RESP75"),
        PARAM=pl.lit("EASI-75 Response"),
        AVALC=avalc,
        ARSN=arsn,
    )
    .with_columns(
        AVAL=pl.when(pl.col("AVALC") == "RESPONDER")
        .then(1)
        .when(pl.col("AVALC") == "NON-RESPONDER")
        .then(0)
        .otherwise(None)
        .cast(pl.Int64)
    )
    .select(
        "STUDYID", "USUBJID", "PARAMCD", "PARAM", "AVISIT", "PCHG",
        "SAEFL", "DCSREAS", "AVALC", "ARSN", "AVAL",
    )
    .sort(["USUBJID", "AVISIT"])
)

Path("/app/output").mkdir(exist_ok=True)
adrs.write_csv("/app/output/adrs.csv")
