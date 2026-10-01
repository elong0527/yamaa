# Reference solution for the yamaa benchmark adam-advs-map (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

raw = pl.read_csv("/app/input/advs.csv", infer_schema=False).with_columns(
    pl.col("AVAL").cast(pl.Float64, strict=False)
)

# Every collected record stays in place.
collected = raw.select(
    "STUDYID", "USUBJID", "PARAMCD", "PARAM", "AVAL", "VISIT",
    DTYPE=pl.lit(None, dtype=pl.String),
)

# The visit's systolic and diastolic results.
sysbp = (
    raw.filter(
        (pl.col("PARAMCD") == "SYSBP") & pl.col("AVAL").is_not_null()
    )
    .group_by(["STUDYID", "USUBJID", "VISIT"])
    .agg(SYSBP_VAL=pl.col("AVAL").first())
)
diabp = (
    raw.filter(
        (pl.col("PARAMCD") == "DIABP") & pl.col("AVAL").is_not_null()
    )
    .group_by(["STUDYID", "USUBJID", "VISIT"])
    .agg(DIABP_VAL=pl.col("AVAL").first())
)

# One mean arterial pressure record for each visit with both results,
# counting the diastolic pressure twice.
map_records = (
    sysbp.join(diabp, on=["STUDYID", "USUBJID", "VISIT"], how="inner")
    .with_columns(AVAL=(pl.col("SYSBP_VAL") + 2 * pl.col("DIABP_VAL")) / 3)
    .select(
        "STUDYID",
        "USUBJID",
        "AVAL",
        "VISIT",
        PARAMCD=pl.lit("MAP"),
        PARAM=pl.lit("Mean Arterial Pressure (mmHg)"),
        DTYPE=pl.lit("CALCULATION"),
    )
    .select("STUDYID", "USUBJID", "PARAMCD", "PARAM", "AVAL", "VISIT", "DTYPE")
)

advs = pl.concat([collected, map_records], how="diagonal").sort(
    ["USUBJID", "VISIT", "PARAMCD"]
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
advs.write_csv("/app/output/advs.csv")
