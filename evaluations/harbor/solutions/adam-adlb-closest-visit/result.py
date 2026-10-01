# Reference solution for the yamaa benchmark adam-adlb-closest-visit (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

raw = pl.read_csv("/app/input/adlb_raw.csv", infer_schema=False).with_columns(
    pl.col("LBSEQ").cast(pl.Int64),
    pl.col("ADY").cast(pl.Int64, strict=False),
    pl.col("AVAL").cast(pl.Float64, strict=False),
)

# WEEK 2 for study days 8 through 22; target day 15 and distance travel
# together, and records outside the window or with a missing day carry
# none and are never flagged.
based = raw.with_columns(
    AVISIT=pl.when(
        pl.col("ADY").is_not_null()
        & (pl.col("ADY") >= 8)
        & (pl.col("ADY") <= 22)
    )
    .then(pl.lit("WEEK 2"))
    .otherwise(None),
).with_columns(
    AWTARGET=pl.when(pl.col("AVISIT").is_not_null())
    .then(15)
    .otherwise(None)
    .cast(pl.Int64),
).with_columns(
    ADIST=(pl.col("ADY") - pl.col("AWTARGET")).abs().cast(pl.Float64),
)

# The record standing for its subject and parameter: the closest to the
# target, the later study day when equally close, the lower sequence
# number when they share the same day.
winners = (
    based.filter(pl.col("AVISIT").is_not_null())
    .sort(
        ["ADIST", "ADY", "LBSEQ"],
        descending=[False, True, False],
        nulls_last=True,
    )
    .unique(
        ["STUDYID", "USUBJID", "PARAMCD", "AVISIT"],
        keep="first",
        maintain_order=True,
    )
    .select("STUDYID", "USUBJID", "PARAMCD", "AVISIT", "LBSEQ")
    .with_columns(pl.lit("Y").alias("ANL01FL"))
)

adlb = (
    based.join(
        winners,
        on=["STUDYID", "USUBJID", "PARAMCD", "AVISIT", "LBSEQ"],
        how="left",
        maintain_order="left",
    )
    .select(
        "STUDYID",
        "USUBJID",
        "PARAMCD",
        "LBSEQ",
        "ADT",
        "ADY",
        "AVAL",
        "AVISIT",
        "AWTARGET",
        "ADIST",
        "ANL01FL",
    )
)

Path("/app/output").mkdir(exist_ok=True)
adlb.write_csv("/app/output/adlb.csv")
