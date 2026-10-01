# Reference solution for the yamaa benchmark adam-adae-severity-rank (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

ae = pl.read_csv("/app/input/ae.csv", infer_schema=False).with_columns(
    pl.col("AESEQ").cast(pl.Int64)
)

asevn = (
    pl.when(pl.col("AESEV") == "MILD")
    .then(1)
    .when(pl.col("AESEV") == "MODERATE")
    .then(2)
    .when(pl.col("AESEV") == "SEVERE")
    .then(3)
    .otherwise(None)
    .cast(pl.Int64)
)

base = ae.with_columns(ASEV=pl.col("AESEV"), ASEVN=asevn)

# Competition rank (worst first, gaps after ties) and dense rank of the
# distinct severities. Polars ranks only non-null values, so unreported
# events take one rank after every reported one.
rank_min = pl.col("ASEVN").rank("min", descending=True).over("USUBJID")
rank_dense = pl.col("ASEVN").rank("dense", descending=True).over("USUBJID")
n_reported = pl.col("ASEVN").count().over("USUBJID")
max_dense = rank_dense.max().over("USUBJID")

adae = (
    base.with_columns(
        SEVRANK=(rank_min.fill_null(n_reported + 1)).cast(pl.Int64),
        SEVLVL=(rank_dense.fill_null(max_dense.fill_null(0) + 1)).cast(pl.Int64),
    ).select("STUDYID", "USUBJID", "AESEQ", "ASEV", "ASEVN", "SEVRANK", "SEVLVL")
)

Path("/app/output").mkdir(exist_ok=True)
adae.write_csv("/app/output/adae.csv")
