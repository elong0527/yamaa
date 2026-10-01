# Reference solution for the yamaa benchmark sdtm-dm-metadata (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

dm_raw = pl.read_csv("/app/input/dm_raw.csv", infer_schema=False)

dm = (
    dm_raw.with_columns(
        DOMAIN=pl.lit("DM"),
        USUBJID=pl.col("STUDYID") + pl.lit("-") + pl.col("SITEID") + pl.lit("-") + pl.col("SUBJID"),
        AGE=pl.col("AGE").cast(pl.Int64, strict=False),
        AGEU=pl.lit("YEARS"),
    )
    .select(
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "SUBJID",
        "SITEID",
        "AGE",
        "AGEU",
        "SEX",
        "COUNTRY",
    )
)

Path("/app/output").mkdir(exist_ok=True)
dm.write_csv("/app/output/dm.csv")
