# Reference solution for the yamaa benchmark adam-adlb-ordered-sum (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

lb = pl.read_csv("/app/input/lb.csv", infer_schema=False).with_columns(
    pl.col("LBSEQ").cast(pl.Int64),
    pl.col("LBSTRESN").cast(pl.Float64, strict=False).alias("AVAL"),
)

# Keep every collected component record; DTYPE has no value on these.
components = lb.select(
    "STUDYID",
    "USUBJID",
    pl.col("LBTESTCD").alias("PARAMCD"),
    pl.col("LBTEST").alias("PARAM"),
    pl.col("VISIT").alias("AVISIT"),
    "AVAL",
    pl.lit(None, dtype=pl.String).alias("DTYPE"),
)


# One total record per subject and visit: the sum of the collected
# component results in laboratory sequence order. No value, rather than
# zero, when none of them has a collected result.
def ordered_sum(values: list) -> float | None:
    nums = [v for v in values if v is not None]
    if not nums:
        return None
    total = nums[0]
    for value in nums[1:]:
        total = total + value
    return total


totals = (
    lb.sort(["USUBJID", "VISIT", "LBSEQ"])
    .group_by("STUDYID", "USUBJID", "VISIT", maintain_order=True)
    .agg(pl.col("AVAL").alias("VALS"))
    .with_columns(
        PARAMCD=pl.lit("TOTAL"),
        PARAM=pl.lit("Total of Components"),
        AVISIT=pl.col("VISIT"),
        AVAL=pl.col("VALS").map_elements(
            ordered_sum, return_dtype=pl.Float64
        ),
        DTYPE=pl.lit("CALCULATION"),
    )
    .select("STUDYID", "USUBJID", "PARAMCD", "PARAM", "AVISIT", "AVAL", "DTYPE")
)

adlb = pl.concat([components, totals], how="diagonal")

Path("/app/output").mkdir(exist_ok=True)
adlb.write_csv("/app/output/adlb.csv")
