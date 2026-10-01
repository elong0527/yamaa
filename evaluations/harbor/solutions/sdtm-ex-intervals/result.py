# Reference solution for the yamaa benchmark sdtm-ex-intervals (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

ec_raw = pl.read_csv("/app/input/ec_raw.csv", infer_schema=False)

ec = ec_raw.with_columns(
    EXDOSE_NUM=pl.col("ECDOSE").cast(pl.Float64, strict=False),
    ECSTDTC_DT=pl.col("ECSTDTC").str.to_date(strict=False),
    ECADJ_CLEAN=pl.col("ECADJ").map_elements(
        lambda v: None if v is None or v == "" else v, return_dtype=pl.String
    ),
)

grouped = (
    ec.group_by("STUDYID", "USUBJID", "ECTRT", "EXDOSE_NUM", "ECDOSU", "ECDOSFRQ")
    .agg(
        pl.col("ECSTDTC_DT").min().alias("START_DT"),
        pl.col("ECSTDTC_DT").max().alias("END_DT"),
        pl.col("ECADJ_CLEAN").drop_nulls().unique().sort().first().alias("EXADJ"),
        pl.col("ECDOSE").first().alias("EXDOSE_RAW"),
    )
    .with_columns(
        DOMAIN=pl.lit("EX"),
        EXTRT=pl.col("ECTRT"),
        EXDOSE=pl.col("EXDOSE_NUM"),
        EXDOSU=pl.col("ECDOSU"),
        EXDOSFRQ=pl.col("ECDOSFRQ"),
        EXSTDTC=pl.col("START_DT"),
        EXENDTC=pl.col("END_DT"),
    )
    .sort(["STUDYID", "USUBJID", "START_DT", "EXTRT"])
    .with_columns(EXSEQ=pl.col("USUBJID").cum_count().over(["STUDYID", "USUBJID"]))
    .with_columns(EXSEQ=pl.col("EXSEQ").cast(pl.Int64))
    .select(
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "EXSEQ",
        "EXTRT",
        "EXDOSE",
        "EXDOSU",
        "EXDOSFRQ",
        "EXSTDTC",
        "EXENDTC",
        "EXADJ",
    )
)

Path("/app/output").mkdir(exist_ok=True)
grouped.write_csv("/app/output/ex.csv")
