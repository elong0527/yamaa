# Reference solution for the yamaa benchmark adam-adex-cumulative-dose (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

ex = pl.read_csv("/app/input/ex.csv", infer_schema=False)
trt = pl.read_csv("/app/input/subject_treatment.csv", infer_schema=False).with_columns(
    pl.col("PLANDOSE").cast(pl.Float64, strict=False),
    pl.col("PLANCYC").cast(pl.Int64, strict=False),
)

# Every exposure record counts, even one with a zero or missing dose; a
# missing dose adds nothing. A duplicated record counts once per entry.
doses = (
    ex.with_columns(EXDOSE_NUM=pl.col("EXDOSE").cast(pl.Float64, strict=False))
    .group_by(["USUBJID", "EXTRT"])
    .agg(
        DOSECUM=pl.col("EXDOSE_NUM").sum(),
        NCYCLES=pl.len().cast(pl.Int64),
    )
)

adex = (
    trt.join(doses, on=["USUBJID", "EXTRT"], how="left")
    .with_columns(PLANTOT=pl.col("PLANDOSE") * pl.col("PLANCYC"))
    # Not rounded; no value when the planned total is zero or when there is
    # no cumulative dose to compare.
    .with_columns(
        RDI=pl.when(pl.col("DOSECUM").is_null())
        .then(None)
        .when(pl.col("PLANTOT").is_null() | (pl.col("PLANTOT") == 0))
        .then(None)
        .otherwise(pl.col("DOSECUM") / pl.col("PLANTOT") * 100)
    )
    .select("STUDYID", "USUBJID", "EXTRT", "EXDOSU", "DOSECUM", "NCYCLES", "RDI")
)

Path("/app/output").mkdir(exist_ok=True)
adex.write_csv("/app/output/adex.csv")
