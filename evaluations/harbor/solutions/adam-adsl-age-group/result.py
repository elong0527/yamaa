# Reference solution for the yamaa benchmark adam-adsl-age-group (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

dm = pl.read_csv("/app/input/dm.csv", infer_schema=False)

age = pl.col("AGE").cast(pl.Float64)
agegr1 = (
    pl.when(age.is_null())
    .then(pl.lit("Missing"))
    .when(age < 18)
    .then(pl.lit("<18"))
    .when(age <= 64)
    .then(pl.lit("18-64"))
    .otherwise(pl.lit(">64"))
)
agegr1n = pl.col("AGEGR1").replace_strict(
    {"<18": 1, "18-64": 2, ">64": 3}, default=None, return_dtype=pl.Int64
)

adsl = (
    dm.with_columns(AGEGR1=agegr1)
    .with_columns(AGEGR1N=agegr1n)
    .select("STUDYID", "USUBJID", "AGE", "AGEU", "AGEGR1", "AGEGR1N")
)

Path("/app/output").mkdir(exist_ok=True)
adsl.write_csv("/app/output/adsl.csv")
