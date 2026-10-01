# Reference solution for the yamaa benchmark adam-adsl-completion-flag (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

adsl_raw = pl.read_csv("/app/input/adsl_raw.csv", infer_schema=False)
ds = pl.read_csv("/app/input/ds.csv", infer_schema=False)

completed = (
    ds.filter(
        (pl.col("DSCAT") == "DISPOSITION EVENT")
        & (pl.col("EPOCH") == "FOLLOW-UP")
        & (pl.col("DSDECOD") == "COMPLETED")
    )
    .select("STUDYID", "USUBJID")
    .unique()
    .with_columns(__completed=pl.lit("Y"))
)

adsl = (
    adsl_raw.join(completed, on=["STUDYID", "USUBJID"], how="left", maintain_order="left")
    .with_columns(
        COMPLFL=pl.when(pl.col("__completed") == "Y")
        .then(pl.lit("Y"))
        .otherwise(pl.lit("N"))
    )
    .select("STUDYID", "USUBJID", "TRTSDT", "COMPLFL")
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
adsl.write_csv("/app/output/adsl.csv")
