# Reference solution for the yamaa benchmark adam-adsl-flag-chain (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

dm = pl.read_csv("/app/input/dm.csv", infer_schema=False).with_columns(
    pl.col("RANDDT").str.to_date(strict=False),
    pl.col("AGE").cast(pl.Int64, strict=False),
)
ex = pl.read_csv("/app/input/ex.csv", infer_schema=False).with_columns(
    pl.col("EXSTDTC").str.to_date(strict=False),
)

first_exposure = ex.group_by(["STUDYID", "USUBJID"]).agg(
    TRTSDT=pl.col("EXSTDTC").min()
)

adsl = (
    dm.join(first_exposure, on=["STUDYID", "USUBJID"], how="left", maintain_order="left")
    .with_columns(
        SAFFL=pl.when(pl.col("TRTSDT").is_not_null())
        .then(pl.lit("Y"))
        .otherwise(pl.lit("N")),
        RANDFL=pl.when(pl.col("RANDDT").is_not_null())
        .then(pl.lit("Y"))
        .otherwise(pl.lit("N")),
    )
    .with_columns(
        ITTFL=pl.when(pl.col("RANDFL") == "Y")
        .then(pl.lit("Y"))
        .otherwise(pl.lit("N")),
    )
    .with_columns(
        POPFL=pl.when((pl.col("SAFFL") == "Y") & (pl.col("ITTFL") == "Y"))
        .then(pl.lit("Y"))
        .otherwise(pl.lit("N")),
        AGEGR1=pl.when(pl.col("AGE") < 65)
        .then(pl.lit("<65"))
        .otherwise(pl.lit(">=65")),
    )
)

# Rank by age within the study, youngest first, USUBJID breaking ties.
ranked = (
    adsl.with_columns(__idx=pl.int_range(pl.len()))
    .sort(["STUDYID", "AGE", "USUBJID"], nulls_last=True)
    .with_columns(AGERNK=pl.int_range(1, pl.len() + 1).over("STUDYID"))
    .sort("__idx")
    .drop("__idx")
    .select(
        "POPFL",
        "SAFFL",
        "ITTFL",
        "TRTSDT",
        "RANDDT",
        "AGEGR1",
        "AGERNK",
        "STUDYID",
        "USUBJID",
        "AGE",
    )
)

Path("/app/output").mkdir(exist_ok=True)
ranked.write_csv("/app/output/adsl.csv")
