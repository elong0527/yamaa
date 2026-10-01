# Reference solution for the yamaa benchmark adam-adsl-treatment (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

dm = pl.read_csv("/app/input/dm.csv", infer_schema=False, null_values="")
ex = pl.read_csv("/app/input/ex.csv", infer_schema=False, null_values="").with_columns(
    pl.col("EXSEQ").cast(pl.Int64, strict=False),
)

qual = ["VITAMIN D3", "PLACEBO"]

# A start collected as a date alone reads midnight; an end collected as a
# date alone reads the last moment of the day.
ex_dt = ex.with_columns(
    __start_dt=pl.col("EXSTDTC").str.to_datetime(strict=False),
    __end_base=pl.col("EXENDTC").str.to_datetime(strict=False),
).with_columns(
    __end_dt=pl.when(pl.col("EXENDTC").str.contains("T"))
    .then(pl.col("__end_base"))
    .when(pl.col("EXENDTC").str.len_chars() == 10)
    .then(pl.col("__end_base") + pl.duration(hours=23, minutes=59, seconds=59))
    .otherwise(pl.col("__end_base")),
)

start_qual = ex_dt.filter(
    pl.col("EXTRT").is_in(qual) & pl.col("__start_dt").is_not_null()
).sort(["__start_dt", "EXSEQ"])
first_start = start_qual.unique(["STUDYID", "USUBJID"], keep="first", maintain_order=True).select(
    "STUDYID", "USUBJID", TRT01RAW="EXTRT", TRTSDTM_DT="__start_dt", __first_std="EXSTDTC"
)

end_qual = ex_dt.filter(
    pl.col("EXTRT").is_in(qual) & pl.col("__end_dt").is_not_null()
).sort(["__end_dt"], descending=True)
last_end = end_qual.unique(["STUDYID", "USUBJID"], keep="first", maintain_order=True).select(
    "STUDYID", "USUBJID", TRTEDTM_DT="__end_dt", __last_endtc="EXENDTC"
)

adsl = (
    dm.join(first_start, on=["STUDYID", "USUBJID"], how="left", maintain_order="left")
    .join(last_end, on=["STUDYID", "USUBJID"], how="left")
    .with_columns(
        __src=pl.coalesce("TRT01RAW", "ACTARM"),
    )
    .with_columns(
        TRT01A=pl.when(pl.col("__src").is_null())
        .then(pl.lit("NOT TREATED"))
        .otherwise(pl.col("__src").str.to_uppercase()),
        TRTSDTM=pl.col("TRTSDTM_DT").dt.strftime("%Y-%m-%dT%H:%M:%S"),
        TRTSDT=pl.col("TRTSDTM_DT").cast(pl.Date),
        TRTSTMF=pl.when(
            pl.col("__first_std").is_not_null() & ~pl.col("__first_std").str.contains("T")
        )
        .then(pl.lit("H"))
        .otherwise(None),
        TRTEDTM=pl.col("TRTEDTM_DT").dt.strftime("%Y-%m-%dT%H:%M:%S"),
        TRTEDT=pl.col("TRTEDTM_DT").cast(pl.Date),
        TRTETMF=pl.when(
            pl.col("__last_endtc").is_not_null() & ~pl.col("__last_endtc").str.contains("T")
        )
        .then(pl.lit("H"))
        .otherwise(None),
    )
    .with_columns(
        TRTDURD=(pl.col("TRTEDT") - pl.col("TRTSDT")).dt.total_days() + 1,
        SAFFL=pl.when(pl.col("TRTSDT").is_not_null())
        .then(pl.lit("Y"))
        .otherwise(pl.lit("N")),
    )
    .select(
        "STUDYID",
        "USUBJID",
        "TRT01A",
        "TRTSDT",
        "TRTSDTM",
        "TRTSTMF",
        "TRTEDT",
        "TRTEDTM",
        "TRTETMF",
        "TRTDURD",
        "SAFFL",
    )
    .sort("USUBJID")
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
adsl.write_csv("/app/output/adsl.csv", null_value="")
