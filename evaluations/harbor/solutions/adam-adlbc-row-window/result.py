# Reference solution for the yamaa benchmark adam-adlbc-row-window (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

lb = pl.read_csv("/app/input/lb.csv", infer_schema=False).with_columns(
    pl.col("LBSTRESN").cast(pl.Float64, strict=False).alias("AVAL"),
    pl.col("VISITNUM").cast(pl.Int64, strict=False).alias("AVISITN"),
)

# Build the change parameters from albumin and bilirubin; any other test
# produces no rows. A record with no numeric result gets no row.
param_map = {"ALB": "_ALB", "BILI": "_BILI"}
filtered = (
    lb.filter(
        pl.col("LBTESTCD").is_in(list(param_map.keys()))
        & pl.col("AVAL").is_not_null()
        & pl.col("AVISITN").is_not_null()
    )
    .with_columns(
        PARAMCD=pl.col("LBTESTCD").replace_strict(param_map),
    )
    .select("STUDYID", "USUBJID", "PARAMCD", "AVISITN", "AVAL")
    .sort(["USUBJID", "PARAMCD", "AVISITN"])
)

# The previous visit's value for the same subject and parameter, and the
# change since that visit.
adbc = (
    filtered.with_columns(
        PREV_AVAL=pl.col("AVAL").shift(1).over(["USUBJID", "PARAMCD"]),
    )
    .with_columns(
        CHG=pl.col("AVAL") - pl.col("PREV_AVAL"),
    )
    .select("STUDYID", "USUBJID", "PARAMCD", "AVISITN", "AVAL", "PREV_AVAL", "CHG")
)

Path("/app/output").mkdir(exist_ok=True)
adbc.write_csv("/app/output/adlbc.csv")
