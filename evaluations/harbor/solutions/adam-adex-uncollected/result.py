# Reference solution for the yamaa benchmark adam-adex-uncollected (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

trt = pl.read_csv("/app/input/subject_treatment.csv", infer_schema=False)
ex = pl.read_csv("/app/input/ex.csv", infer_schema=False).with_columns(
    pl.col("EXDOSE").cast(pl.Float64, strict=False)
)

# Per treatment: total dose, record count, and count of recorded doses.
agg = (
    ex.group_by("STUDYID", "USUBJID", "EXTRT")
    .agg(
        pl.col("EXDOSE").sum().alias("DOSE_SUM"),
        pl.len().cast(pl.Int64).alias("NDOSREC"),
        pl.col("EXDOSE").count().cast(pl.Int64).alias("NDOSVAL"),
    )
    .with_columns(
        DOSECUM=pl.when(pl.col("NDOSVAL") > 0)
        .then(pl.col("DOSE_SUM"))
        .otherwise(None)
    )
    .select("STUDYID", "USUBJID", "EXTRT", "DOSECUM", "NDOSREC", "NDOSVAL")
)

adex = trt.join(
    agg, on=["STUDYID", "USUBJID", "EXTRT"], how="left", maintain_order="left"
).select("STUDYID", "USUBJID", "EXTRT", "DOSECUM", "NDOSREC", "NDOSVAL")

Path("/app/output").mkdir(exist_ok=True)
adex.write_csv("/app/output/adex.csv")
