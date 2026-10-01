# Reference solution for the yamaa benchmark adam-advs-bmi (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

vs = pl.read_csv("/app/input/vs.csv", infer_schema=False).with_columns(
    pl.col("VSSTRESN").cast(pl.Float64, strict=False)
)
adsl = pl.read_csv("/app/input/adsl.csv", infer_schema=False).with_columns(
    pl.col("HEIGHTBL").cast(pl.Float64, strict=False)
)

# Every collected result stays in place.
collected = vs.select(
    "STUDYID",
    "USUBJID",
    AVISIT="VISIT",
    PARAMCD="VSTESTCD",
    PARAM="VSTEST",
    AVAL="VSSTRESN",
)

# One BMI record for each weight with a result, using the subject's
# baseline height in metres. Without a usable height the record stays
# with no value.
weights = (
    vs.filter(
        (pl.col("VSTESTCD") == "WEIGHT") & pl.col("VSSTRESN").is_not_null()
    )
    .join(adsl.select("USUBJID", "HEIGHTBL"), on="USUBJID", how="left")
    .with_columns(
        BMI=pl.when(
            pl.col("HEIGHTBL").is_not_null() & (pl.col("HEIGHTBL") != 0)
        )
        .then(pl.col("VSSTRESN") / ((pl.col("HEIGHTBL") / 100) ** 2))
        .otherwise(None)
    )
    .select(
        "STUDYID",
        "USUBJID",
        AVISIT="VISIT",
        PARAMCD=pl.lit("BMI"),
        PARAM=pl.lit("Body Mass Index (kg/m^2)"),
        AVAL="BMI",
    )
)

advs = pl.concat([collected, weights], how="diagonal").sort(
    ["USUBJID", "AVISIT", "PARAMCD"]
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
advs.write_csv("/app/output/advs.csv")
