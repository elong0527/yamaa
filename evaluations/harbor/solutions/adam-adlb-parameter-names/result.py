# Reference solution for the yamaa benchmark adam-adlb-parameter-names (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

lb = pl.read_csv("/app/input/lb.csv", infer_schema=False)

param = {
    "ALT": "Alanine Aminotransferase (U/L)",
    "AST": "Aspartate Aminotransferase (U/L)",
}
visit = {"1": "Baseline", "2": "Week 2"}
adlb = lb.select(
    pl.col("STUDYID"),
    pl.col("USUBJID"),
    pl.col("LBTESTCD").alias("PARAMCD"),
    pl.col("LBTESTCD").replace_strict(param).alias("PARAM"),
    pl.col("VISITNUM").cast(pl.Int64).alias("AVISITN"),
    pl.col("VISITNUM")
    .replace_strict(visit, default="Unscheduled")
    .alias("AVISIT"),
    pl.col("LBSTRESN").cast(pl.Float64).alias("AVAL"),
)

Path("/app/output").mkdir(exist_ok=True)
adlb.write_csv("/app/output/adlb.csv")
