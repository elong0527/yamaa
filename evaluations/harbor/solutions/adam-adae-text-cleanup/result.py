# Reference solution for the yamaa benchmark adam-adae-text-cleanup (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

ae = pl.read_csv("/app/input/ae.csv", infer_schema=False).with_columns(
    pl.col("AESEQ").cast(pl.Int64)
)

# AE, a hyphen, and exactly three digits; missing is 0, any other shape is
# -1, and the digits give the number.
aerefnum = (
    pl.when(pl.col("AESPID").is_null() | (pl.col("AESPID") == ""))
    .then(0)
    .when(pl.col("AESPID").str.contains(r"^AE-[0-9]{3}$"))
    .then(pl.col("AESPID").str.slice(3).cast(pl.Int64))
    .otherwise(-1)
    .cast(pl.Int64)
)
aerellc = (
    pl.when(pl.col("AEREL").is_null() | (pl.col("AEREL") == ""))
    .then(pl.lit("not reported"))
    .otherwise(pl.col("AEREL").str.to_lowercase())
)

adae = (
    ae.with_columns(
        AEREFNUM=aerefnum,
        AETERMLO=pl.col("AETERM").str.to_lowercase(),
        AERELLC=aerellc,
    )
    .with_columns(AREL=pl.col("AERELLC").str.to_uppercase())
    .select(
        "STUDYID",
        "USUBJID",
        "AESEQ",
        "AESPID",
        "AEREFNUM",
        "AETERM",
        "AETERMLO",
        "AERELLC",
        "AREL",
    )
)

Path("/app/output").mkdir(exist_ok=True)
adae.write_csv("/app/output/adae.csv")
