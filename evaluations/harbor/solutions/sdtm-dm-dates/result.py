# Reference solution for the yamaa benchmark sdtm-dm-dates (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

dm_raw = pl.read_csv("/app/input/dm_raw.csv", infer_schema=False)
ex = pl.read_csv("/app/input/ex.csv", infer_schema=False)
ds = pl.read_csv("/app/input/ds.csv", infer_schema=False)
ae = pl.read_csv("/app/input/ae.csv", infer_schema=False)

ex_sum = (
    ex.with_columns(
        pl.col("EXSTDTC").str.to_date(strict=False),
        pl.col("EXENDTC").str.to_date(strict=False),
    )
    .group_by("STUDYID", "USUBJID")
    .agg(
        pl.col("EXSTDTC").min().alias("RFXSTDTC"),
        pl.col("EXENDTC").max().alias("RFXENDTC"),
    )
)

ds_sum = (
    ds.filter(pl.col("DSCAT") == "DISPOSITION EVENT")
    .with_columns(pl.col("DSSTDTC").str.to_date(strict=False))
    .group_by("STUDYID", "USUBJID")
    .agg(pl.col("DSSTDTC").max().alias("DS_MAX"))
)

ae_sum = (
    ae.with_columns(pl.col("AEENDTC").str.to_date(strict=False))
    .group_by("STUDYID", "USUBJID")
    .agg(pl.col("AEENDTC").max().alias("AE_MAX"))
)

dm = (
    dm_raw.join(ex_sum, on=["STUDYID", "USUBJID"], how="left")
    .join(ds_sum, on=["STUDYID", "USUBJID"], how="left")
    .join(ae_sum, on=["STUDYID", "USUBJID"], how="left")
    .with_columns(
        DOMAIN=pl.lit("DM"),
        RFICDTC=pl.col("RFICDTC").str.to_date(strict=False),
        RFSTDTC=pl.col("RFXSTDTC"),
        RFPENDTC=pl.max_horizontal("RFXENDTC", "DS_MAX", "AE_MAX"),
    )
    .with_columns(
        RFENDTC=pl.when(pl.col("RFSTDTC").is_not_null())
        .then(pl.col("RFPENDTC"))
        .otherwise(None)
    )
    .select(
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "RFICDTC",
        "RFXSTDTC",
        "RFXENDTC",
        "RFSTDTC",
        "RFPENDTC",
        "RFENDTC",
    )
)

Path("/app/output").mkdir(exist_ok=True)
dm.write_csv("/app/output/dm.csv")
