# Reference solution for the yamaa benchmark sdtm-fa-fever (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

vs = pl.read_csv("/app/input/vs.csv", infer_schema=False)

temp = vs.filter(
    (pl.col("VSTESTCD") == "TEMP") & (pl.col("VSCAT") == "REACTOGENICITY")
).with_columns(
    pl.col("VSSEQ").cast(pl.Int64).alias("FASEQ"),
    pl.col("VSSTRESN").cast(pl.Float64, strict=False).alias("RESULTN"),
)

fa = temp.with_columns(
    pl.when(
        pl.col("RESULTN").is_null()
        | pl.col("VSSTRESU").is_null()
        | (pl.col("VSSTRESU") != "C")
    )
    .then(pl.lit(None, dtype=pl.String))
    .when(pl.col("RESULTN") >= 38)
    .then(pl.lit("Y"))
    .otherwise(pl.lit("N"))
    .alias("FAORRES"),
).with_columns(
    DOMAIN=pl.lit("FA"),
    FATESTCD=pl.lit("OCCUR"),
    FATEST=pl.lit("Occurrence Indicator"),
    FACAT=pl.lit("REACTOGENICITY"),
    FASCAT=pl.lit("SYSTEMIC"),
    FAOBJ=pl.lit("FEVER"),
    FASTRESC=pl.col("FAORRES"),
    VSSTRESN=pl.col("RESULTN"),
).select(
    "DOMAIN",
    "STUDYID",
    "USUBJID",
    "FASEQ",
    "FATESTCD",
    "FATEST",
    "FACAT",
    "FASCAT",
    "FAOBJ",
    "FAORRES",
    "FASTRESC",
    "VSSTRESN",
).sort(["STUDYID", "USUBJID", "FASEQ"])

Path("/app/output").mkdir(parents=True, exist_ok=True)
fa.write_csv("/app/output/fa.csv")
