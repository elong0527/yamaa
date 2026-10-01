# Reference solution for the yamaa benchmark adam-adrs-measurable-disease (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

adsl = pl.read_csv("/app/input/adsl.csv", infer_schema=False)
tu = pl.read_csv("/app/input/tu.csv", infer_schema=False).with_columns(
    pl.col("TUSEQ").cast(pl.Int64)
)

# Target disease at screening: a screening tumor identification record
# whose result is target. Later target records do not count.
target_subjects = tu.filter(
    (pl.col("TUTESTCD") == "TUMIDENT")
    & (pl.col("VISIT") == "SCREENING")
    & (pl.col("TUSTRESC") == "TARGET")
).select("USUBJID").unique()

adrs = (
    adsl.with_columns(
        PARAMCD=pl.lit("MDIS"),
        PARAM=pl.lit("Measurable Disease at Baseline"),
        AVALC=pl.when(pl.col("USUBJID").is_in(target_subjects["USUBJID"]))
        .then(pl.lit("Y"))
        .otherwise(pl.lit("N")),
    )
    .with_columns(
        AVAL=pl.when(pl.col("AVALC") == "Y").then(1).otherwise(0).cast(pl.Int64)
    )
    .select("STUDYID", "USUBJID", "PARAMCD", "PARAM", "AVALC", "AVAL")
    .sort("USUBJID")
)

Path("/app/output").mkdir(exist_ok=True)
adrs.write_csv("/app/output/adrs.csv")
