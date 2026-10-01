# Reference solution for the yamaa benchmark adam-adae-review-order (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

ae = pl.read_csv("/app/input/ae.csv", infer_schema=False).with_columns(
    pl.col("AESEQ").cast(pl.Int64)
)

adae = ae.select(
    "STUDYID",
    "USUBJID",
    pl.col("AESEQ").alias("ASEQ"),
    "AETERM",
    pl.col("AESTDTC").alias("ASTDT"),
    pl.col("AESEV").alias("ASEV"),
)

Path("/app/output").mkdir(exist_ok=True)
adae.write_csv("/app/output/adae.csv")
