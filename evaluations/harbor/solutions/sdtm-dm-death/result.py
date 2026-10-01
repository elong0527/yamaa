# Reference solution for the yamaa benchmark sdtm-dm-death (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

dm_raw = pl.read_csv("/app/input/dm_raw.csv", infer_schema=False)
ds = pl.read_csv("/app/input/ds.csv", infer_schema=False)
ae = pl.read_csv("/app/input/ae.csv", infer_schema=False)

ds_death = (
    ds.filter(pl.col("DSDECOD") == "DEATH")
    .with_columns(pl.col("DSSTDTC").str.to_date(strict=False))
    .filter(pl.col("DSSTDTC").is_not_null())
    .group_by("STUDYID", "USUBJID")
    .agg(pl.col("DSSTDTC").max().alias("DS_DTH"))
)

ae_fatal = (
    ae.filter(pl.col("AEOUT") == "FATAL")
    .with_columns(pl.col("AEENDTC").str.to_date(strict=False))
    .filter(pl.col("AEENDTC").is_not_null())
    .group_by("STUDYID", "USUBJID")
    .agg(pl.col("AEENDTC").max().alias("AE_DTH"))
)

dm = (
    dm_raw.join(ds_death, on=["STUDYID", "USUBJID"], how="left")
    .join(ae_fatal, on=["STUDYID", "USUBJID"], how="left")
    .with_columns(
        DOMAIN=pl.lit("DM"),
        DTHDTC=pl.coalesce("DS_DTH", "AE_DTH"),
    )
    .with_columns(
        DTHFL=pl.when(pl.col("DTHDTC").is_not_null())
        .then(pl.lit("Y"))
        .otherwise(None)
    )
    .select("DOMAIN", "STUDYID", "USUBJID", "DTHDTC", "DTHFL")
)

Path("/app/output").mkdir(exist_ok=True)
dm.write_csv("/app/output/dm.csv")
