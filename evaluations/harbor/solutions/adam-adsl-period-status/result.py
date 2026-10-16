# Reference solution for the yamaa benchmark adam-adsl-period-status
# (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

dm = pl.read_csv("/app/input/dm.csv", infer_schema=False)
ex = pl.read_csv("/app/input/ex.csv", infer_schema=False)
ds = pl.read_csv("/app/input/ds.csv", infer_schema=False)


def entered(epoch):
    return (
        ex.filter(pl.col("EPOCH") == epoch)
        .select("STUDYID", "USUBJID")
        .unique()
        .with_columns(pl.lit(1, dtype=pl.Int64).alias("ENTERED"))
    )


def disposition(epoch, name):
    return (
        ds.filter(
            (pl.col("DSCAT") == "DISPOSITION EVENT")
            & (pl.col("EPOCH") == epoch)
        )
        .select("STUDYID", "USUBJID", pl.col("DSDECOD").alias(name))
    )


def status(entered_flag, decod):
    return (
        pl.when(
            pl.col(entered_flag).is_not_null()
            & (pl.col(decod) == "COMPLETED")
        )
        .then(pl.lit("COMPLETED"))
        .when(
            pl.col(entered_flag).is_not_null() & pl.col(decod).is_not_null()
        )
        .then(pl.lit("DISCONTINUED"))
        .when(pl.col(entered_flag).is_not_null())
        .then(pl.lit("ONGOING"))
        .otherwise(None)
    )


adsl = (
    dm.join(
        entered("TREATMENT 1").rename({"ENTERED": "P01EX"}),
        on=["STUDYID", "USUBJID"],
        how="left",
        maintain_order="left",
    )
    .join(
        entered("TREATMENT 2").rename({"ENTERED": "P02EX"}),
        on=["STUDYID", "USUBJID"],
        how="left",
        maintain_order="left",
    )
    .join(
        disposition("TREATMENT 1", "P01DECOD"),
        on=["STUDYID", "USUBJID"],
        how="left",
        maintain_order="left",
    )
    .join(
        disposition("TREATMENT 2", "P02DECOD"),
        on=["STUDYID", "USUBJID"],
        how="left",
        maintain_order="left",
    )
    .with_columns(
        EOP01STT=status("P01EX", "P01DECOD"),
        EOP02STT=status("P02EX", "P02DECOD"),
    )
    .with_columns(
        DCP01RS=pl.when(pl.col("EOP01STT") == "DISCONTINUED")
        .then(pl.col("P01DECOD"))
        .otherwise(None),
        DCP02RS=pl.when(pl.col("EOP02STT") == "DISCONTINUED")
        .then(pl.col("P02DECOD"))
        .otherwise(None),
    )
    .select("STUDYID", "USUBJID", "EOP01STT", "DCP01RS", "EOP02STT", "DCP02RS")
)

Path("/app/output").mkdir(exist_ok=True)
adsl.write_csv("/app/output/adsl.csv")
