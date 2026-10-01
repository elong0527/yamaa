# Reference solution for the yamaa benchmark sdtm-ec-collected-exposure (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

log = pl.read_csv("/app/input/dosing_log.csv", infer_schema=False)

ec = (
    log.with_columns(
        DOMAIN=pl.lit("EC"),
        ECTRT=pl.lit("Study Drug"),
        ECMOOD=pl.lit("PERFORMED"),
        ECOCCUR=pl.col("DOSE_TAKEN"),
        ECREASND=pl.when(pl.col("DOSE_TAKEN") == "Y")
        .then(None)
        .otherwise(pl.col("MISSREAS"))
        .map_elements(lambda v: None if v == "" else v, return_dtype=pl.String),
        ECDOSE=pl.when(pl.col("DOSE_TAKEN") == "Y")
        .then(pl.col("TABLETS").cast(pl.Int64, strict=False))
        .otherwise(None),
        ECDOSU=pl.lit("tablet"),
        ECDOSFRM=pl.lit("TABLET"),
        ECDOSFRQ=pl.lit("QD"),
        ECROUTE=pl.lit("ORAL"),
        ECSTDTC=pl.col("LOGDATE"),
        ECENDTC=pl.col("LOGDATE"),
        ECADJ=pl.col("DOSE_ADJ").map_elements(
            lambda v: None if v == "" or v is None else v, return_dtype=pl.String
        ),
    )
    .with_columns(LOGDATE_DT=pl.col("LOGDATE").str.to_date(strict=False))
    .sort(["STUDYID", "USUBJID", "LOGDATE_DT"])
    .with_columns(ECSEQ=pl.col("USUBJID").cum_count().over(["STUDYID", "USUBJID"]))
    .with_columns(ECSEQ=pl.col("ECSEQ").cast(pl.Int64))
    .select(
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "ECSEQ",
        "ECTRT",
        "ECMOOD",
        "ECOCCUR",
        "ECREASND",
        "ECDOSE",
        "ECDOSU",
        "ECDOSFRM",
        "ECDOSFRQ",
        "ECROUTE",
        "ECSTDTC",
        "ECENDTC",
        "ECADJ",
    )
)

Path("/app/output").mkdir(exist_ok=True)
ec.write_csv("/app/output/ec.csv")
