# Reference solution for the yamaa benchmark adam-adsl-disposition (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

dm = pl.read_csv("/app/input/dm.csv", infer_schema=False)
ds = pl.read_csv("/app/input/ds.csv", infer_schema=False).with_columns(
    pl.col("DSSEQ").cast(pl.Int64, strict=False),
    pl.col("DSSTDTC").str.to_date(strict=False),
)

# The last dated disposition event: latest date, highest sequence on a tie.
last = (
    ds.filter(
        (pl.col("DSCAT") == "DISPOSITION EVENT")
        & pl.col("DSSTDTC").is_not_null()
    )
    .sort(["DSSTDTC", "DSSEQ"], descending=True, nulls_last=True)
    .unique(["STUDYID", "USUBJID"], keep="first", maintain_order=True)
    .select("STUDYID", "USUBJID", EOSDT="DSSTDTC", EOSDECOD="DSDECOD", EOSREAS="DSTERM")
)

adsl = (
    dm.join(last, on=["STUDYID", "USUBJID"], how="left", maintain_order="left")
    .with_columns(
        EOSSTT=pl.when(pl.col("EOSDECOD") == "COMPLETED")
        .then(pl.lit("COMPLETED"))
        .when(pl.col("EOSDECOD").is_not_null())
        .then(pl.lit("DISCONTINUED"))
        .otherwise(pl.lit("ONGOING")),
        DCSREAS=pl.when(
            pl.col("EOSDECOD").is_not_null() & (pl.col("EOSDECOD") != "COMPLETED")
        )
        .then(pl.col("EOSREAS"))
        .otherwise(None),
    )
    .select("STUDYID", "USUBJID", "EOSDT", "EOSDECOD", "EOSREAS", "EOSSTT", "DCSREAS")
)

Path("/app/output").mkdir(exist_ok=True)
adsl.write_csv("/app/output/adsl.csv")
