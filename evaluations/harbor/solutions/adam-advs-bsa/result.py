# Reference solution for the yamaa benchmark adam-advs-bsa (Python track).
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

# The visit's height and weight results; a zero result still counts.
heights = (
    raw.filter(
        (pl.col("PARAMCD") == "HEIGHT") & pl.col("AVAL").is_not_null()
    )
    .group_by(["STUDYID", "USUBJID", "VISIT"])
    .agg(HEIGHT_VAL=pl.col("AVAL").first())
)
weights = (
    raw.filter(
        (pl.col("PARAMCD") == "WEIGHT") & pl.col("AVAL").is_not_null()
    )
    .group_by(["STUDYID", "USUBJID", "VISIT"])
    .agg(WEIGHT_VAL=pl.col("AVAL").first())
)

# One body surface area record for each visit with both results.
bsa = (
    heights.join(weights, on=["STUDYID", "USUBJID", "VISIT"], how="inner")
    .with_columns(AVAL=(pl.col("HEIGHT_VAL") * pl.col("WEIGHT_VAL") / 3600).sqrt())
    .select(
        "STUDYID",
        "USUBJID",
        "AVAL",
        "VISIT",
        PARAMCD=pl.lit("BSA"),
        PARAM=pl.lit("Body Surface Area (m^2)"),
        DTYPE=pl.lit("CALCULATION"),
    )
    .select("STUDYID", "USUBJID", "PARAMCD", "PARAM", "AVAL", "VISIT", "DTYPE")
)

advs = pl.concat([collected, bsa], how="diagonal").sort(
    ["USUBJID", "VISIT", "PARAMCD"]
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
advs.write_csv("/app/output/advs.csv")
