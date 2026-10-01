# Reference solution for the yamaa benchmark adam-adsl-country-region (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

dm = pl.read_csv("/app/input/dm.csv", infer_schema=False)

adsl = (
    dm.with_columns(
        COUNTRY=pl.col("COUNTRY").fill_null("").str.strip_chars().str.to_uppercase()
    )
    .with_columns(
        COUNTRY=pl.when(pl.col("COUNTRY") == "")
        .then(pl.lit("UNKNOWN"))
        .otherwise(pl.col("COUNTRY"))
    )
    .with_columns(
        REGION1=pl.when(pl.col("COUNTRY").is_in(["USA", "CAN"]))
        .then(pl.lit("North America"))
        .when(pl.col("COUNTRY") == "DEU")
        .then(pl.lit("Europe"))
        .otherwise(pl.lit("Rest of World"))
    )
    .select("STUDYID", "USUBJID", "COUNTRY", "REGION1")
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
adsl.write_csv("/app/output/adsl.csv")
