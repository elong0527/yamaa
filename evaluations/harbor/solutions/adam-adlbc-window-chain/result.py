# Reference solution for the yamaa benchmark adam-adlbc-window-chain (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

lb = pl.read_csv("/app/input/lb.csv", infer_schema=False).with_columns(
    pl.col("LBSTRESN").cast(pl.Float64),
    pl.col("VISITNUM").cast(pl.Int64),
)

# Albumin results with a numeric value; a record without one gets no row,
# so the lag compares against the latest earlier visit with a result.
alb = (
    lb.filter(pl.col("LBTESTCD") == "ALB", pl.col("LBSTRESN").is_not_null())
    .with_columns(
        PARAMCD=pl.lit("_ALB"),
        AVISITN=pl.col("VISITNUM"),
        AVAL=pl.col("LBSTRESN"),
    )
    .sort(["STUDYID", "USUBJID", "AVISITN"])
)

# The change since the previous visit with a result, and its lag: the
# previous visit's change. A zero change is a real value.
adlbc = (
    alb.with_columns(
        PREV_AVAL=pl.col("AVAL").shift(1).over(["STUDYID", "USUBJID"]),
    )
    .with_columns(CHG=pl.col("AVAL") - pl.col("PREV_AVAL"))
    .with_columns(PREV2=pl.col("CHG").shift(1).over(["STUDYID", "USUBJID"]))
    .select(
        "STUDYID", "USUBJID", "PARAMCD", "AVISITN",
        "AVAL", "PREV_AVAL", "CHG", "PREV2",
    )
)

Path("/app/output").mkdir(exist_ok=True)
adlbc.write_csv("/app/output/adlbc.csv")
