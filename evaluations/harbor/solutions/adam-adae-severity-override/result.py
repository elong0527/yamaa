# Reference solution for the yamaa benchmark adam-adae-severity-override (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

ae = pl.read_csv("/app/input/ae.csv", infer_schema=False).with_columns(
    pl.col("AESEQ").cast(pl.Int64)
)

asev = pl.col("AESEV").str.to_uppercase()
# An approved correction reassigns this one event to SEVERE.
asev = (
    pl.when((pl.col("USUBJID") == "CATH-01-001") & (pl.col("AESEQ") == 2))
    .then(pl.lit("SEVERE"))
    .otherwise(asev)
)
asevn = (
    pl.when(pl.col("ASEV") == "MILD")
    .then(1)
    .when(pl.col("ASEV") == "MODERATE")
    .then(2)
    .when(pl.col("ASEV") == "SEVERE")
    .then(3)
    .when(pl.col("ASEV") == "LIFE-THREATENING")
    .then(4)
    .otherwise(None)
    .cast(pl.Int64)
)

adae = (
    ae.with_columns(ASEV=asev)
    .with_columns(ASEVN=asevn)
    .select("STUDYID", "USUBJID", "AESEQ", "ASEV", "ASEVN")
)

Path("/app/output").mkdir(exist_ok=True)
adae.write_csv("/app/output/adae.csv")
