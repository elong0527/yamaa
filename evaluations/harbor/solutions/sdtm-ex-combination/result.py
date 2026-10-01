# Reference solution for the yamaa benchmark sdtm-ex-combination (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

ex_raw = pl.read_csv("/app/input/ex_raw.csv", infer_schema=False)

ex = (
    ex_raw.with_columns(
        DOMAIN=pl.lit("EX"),
        EXDOSE=pl.col("EXDOSE").cast(pl.Float64, strict=False),
        EXSTDTC_DT=pl.col("EXSTDTC").str.to_date(strict=False),
        EXADJ=pl.col("EXADJ").map_elements(
            lambda v: None if v == "" or v is None else v, return_dtype=pl.String
        ),
    )
    .sort(["STUDYID", "USUBJID", "EXSTDTC_DT", "EXTRT"])
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
        "EXSTDTC",
        "EXENDTC",
        "EXADJ",
    )
)

Path("/app/output").mkdir(exist_ok=True)
ex.write_csv("/app/output/ex.csv")
