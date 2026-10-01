# Reference solution for the yamaa benchmark adam-adsl-demographics (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

dm = pl.read_csv("/app/input/dm.csv", infer_schema=False)

age_num = pl.col("AGE").cast(pl.Float64, strict=False)

adsl = (
    dm.with_columns(SEX=pl.col("SEX").fill_null("").str.strip_chars())
    .with_columns(
        SEX=pl.when(pl.col("SEX") == "")
        .then(pl.lit("U"))
        .otherwise(pl.col("SEX"))
    )
    .with_columns(
        SEXN=pl.col("SEX").replace_strict(
            {"M": 1, "F": 2, "U": 0}, default=None, return_dtype=pl.Int64
        )
    )
    .with_columns(
        RACEN=pl.when(pl.col("RACE").is_null() | (pl.col("RACE") == ""))
        .then(None)
        .when(pl.col("RACE") == "WHITE")
        .then(1)
        .when(pl.col("RACE") == "BLACK OR AFRICAN AMERICAN")
        .then(2)
        .when(pl.col("RACE") == "ASIAN")
        .then(3)
        .when(pl.col("RACE") == "MULTIPLE")
        .then(4)
        .otherwise(99)
        .cast(pl.Int64)
    )
    .with_columns(
        AGE=pl.when(age_num.is_null())
        .then(None)
        .when(age_num.floor() == age_num)
        .then(age_num.cast(pl.Int64))
        .otherwise(None)
    )
    .with_columns(
        AGEGR1=pl.when(pl.col("AGE").is_null())
        .then(pl.lit("UNKNOWN"))
        .when(pl.col("AGE") < 18)
        .then(pl.lit("<18"))
        .when(pl.col("AGE") < 65)
        .then(pl.lit("18-64"))
        .otherwise(pl.lit(">=65"))
    )
    .select("STUDYID", "USUBJID", "SEX", "SEXN", "RACE", "RACEN", "AGE", "AGEGR1")
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
adsl.write_csv("/app/output/adsl.csv")
