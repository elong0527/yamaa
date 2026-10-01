# Reference solution for the yamaa benchmark adam-adlb-reported-precision (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

lb = pl.read_csv("/app/input/lb.csv", infer_schema=False).with_columns(
    pl.col("AVAL").cast(pl.Float64, strict=False),
    pl.col("ANRLO").cast(pl.Float64, strict=False).alias("ANRLO_NUM"),
)

# Keep the collected AVAL/ANRLO as numbers; the ratio is rounded once to
# four places with half away from zero.
ratio = pl.col("AVAL") / pl.col("ANRLO_NUM")
rounded = (
    pl.when(ratio >= 0)
    .then(((ratio * 10000) + 0.5).floor() / 10000)
    .otherwise(((ratio * 10000) - 0.5).ceil() / 10000)
)

adlb = (
    lb.with_columns(
        ANRLO=pl.col("ANRLO_NUM"),
        R2ANRLO=pl.when(
            pl.col("AVAL").is_null()
            | pl.col("ANRLO_NUM").is_null()
            | (pl.col("ANRLO_NUM") == 0)
        )
        .then(None)
        .otherwise(rounded),
    )
    .select("STUDYID", "USUBJID", "PARAMCD", "AVAL", "ANRLO", "R2ANRLO")
    .sort("USUBJID")
)

Path("/app/output").mkdir(exist_ok=True)
adlb.write_csv("/app/output/adlb.csv")
