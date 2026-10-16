# Reference solution for the yamaa benchmark adam-adpc-blq-imputation (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

pc = pl.read_csv("/app/input/pc.csv", infer_schema=False).with_columns(
    pl.col("PCSEQ").cast(pl.Int64),
    pl.col("PCSTRESN").cast(pl.Float64, strict=False),
    pl.col("PCTPTNUM").cast(pl.Float64, strict=False),
)

PROFILE = ["STUDYID", "USUBJID", "PCTESTCD", "PCTPTREF"]

# The first and last quantified time points of each profile. A result
# below the limit and a sample not done have no number, so neither moves
# the quantified span.
span = (
    pc.filter(pl.col("PCSTRESN").is_not_null())
    .group_by(PROFILE)
    .agg(
        pl.col("PCTPTNUM").min().alias("FIRSTQ"),
        pl.col("PCTPTNUM").max().alias("LASTQ"),
    )
)

based = pc.join(span, on=PROFILE, how="left", maintain_order="left")

# A quantified result as recorded; a below-limit result is 0 before the
# first or after the last quantified result of its profile (and in a
# profile with no quantified result), and has no value between them. A
# sample not done has no value.
adpc = based.with_columns(
    AVAL=pl.when(pl.col("PCSTRESN").is_not_null())
    .then(pl.col("PCSTRESN"))
    .when(
        pl.col("PCSTRESC").str.starts_with("<")
        & (
            pl.col("FIRSTQ").is_null()
            | (pl.col("PCTPTNUM") < pl.col("FIRSTQ"))
            | (pl.col("PCTPTNUM") > pl.col("LASTQ"))
        )
    )
    .then(pl.lit(0.0))
    .otherwise(None),
).select(
    "STUDYID",
    "USUBJID",
    "PCSEQ",
    pl.col("PCTESTCD").alias("PARAMCD"),
    pl.col("PCTPTREF").alias("ATPTREF"),
    pl.col("PCTPT").alias("ATPT"),
    pl.col("PCTPTNUM").alias("ATPTN"),
    "AVAL",
    pl.col("PCSTRESU").alias("AVALU"),
)

Path("/app/output").mkdir(exist_ok=True)
adpc.write_csv("/app/output/adpc.csv")
