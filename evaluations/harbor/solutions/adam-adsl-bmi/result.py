# Reference solution for the yamaa benchmark adam-adsl-bmi (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

src = pl.read_csv("/app/input/adsl.csv", infer_schema=False)

height = pl.col("HEIGHTCM").cast(pl.Float64, strict=False)
weight = pl.col("WEIGHTKG").cast(pl.Float64, strict=False)
bmi = (
    pl.when(height.is_null() | (height == 0) | weight.is_null())
    .then(None)
    .otherwise(weight / ((height / 100).pow(2)))
)

adsl = src.with_columns(BMI=bmi, BMI_FN=bmi).select(
    "STUDYID", "USUBJID", "HEIGHTCM", "WEIGHTKG", "BMI", "BMI_FN"
)

Path("/app/output").mkdir(exist_ok=True)
adsl.write_csv("/app/output/adsl.csv")
