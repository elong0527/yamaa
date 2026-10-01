# Reference solution for the yamaa benchmark adam-adlb-mean (Python track).
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

# Mean of the subject's collected values for the parameter, repeated on
# every record for that subject and parameter.
means = (
    lb.filter(pl.col("AVAL").is_not_null())
    .group_by("STUDYID", "USUBJID", "LBTESTCD")
    .agg(pl.col("AVAL").mean().alias("AVALMEAN"))
)

adlb = (
    lb.join(
        means, on=["STUDYID", "USUBJID", "LBTESTCD"], how="left"
    )
    .select("STUDYID", "USUBJID", "LBSEQ", "LBTESTCD", "AVAL", "AVALMEAN")
    .rename({"LBTESTCD": "PARAMCD"})
    .select("STUDYID", "USUBJID", "LBSEQ", "PARAMCD", "AVAL", "AVALMEAN")
    .sort(["USUBJID", "LBSEQ"])
)

Path("/app/output").mkdir(exist_ok=True)
adlb.write_csv("/app/output/adlb.csv")
