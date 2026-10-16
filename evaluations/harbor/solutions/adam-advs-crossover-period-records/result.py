# Reference solution for the yamaa benchmark
# adam-advs-crossover-period-records (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

vs = pl.read_csv("/app/input/vs.csv", infer_schema=False).with_columns(
    pl.col("VSSEQ").cast(pl.Int64, strict=False),
    pl.col("VSSTRESN").cast(pl.Float64, strict=False),
    # The collection date is the date part of the collection datetime;
    # the datetime itself exists only when a time was collected.
    ADT=pl.col("VSDTC").str.slice(0, 10).str.to_date(strict=False),
    ADTM=pl.when(pl.col("VSDTC").str.contains("T"))
    .then(pl.col("VSDTC").str.to_datetime(strict=False))
    .otherwise(None),
)
adsl = pl.read_csv("/app/input/adsl.csv", infer_schema=False).with_columns(
    pl.col("AP01SDT", "AP02SDT").str.to_date(strict=False),
    pl.col("AP01SDTM", "AP02SDTM").str.to_datetime(strict=False),
)

# The latest period whose start is at or before the record. A record
# with a time is compared with the period start datetimes, a date-only
# record with the period start dates; a missing start never matches,
# so a subject who never started period 2 stays in period 1.
timed2 = (
    pl.col("ADTM").is_not_null()
    & pl.col("AP02SDTM").is_not_null()
    & (pl.col("ADTM") >= pl.col("AP02SDTM"))
)
dated2 = (
    pl.col("ADTM").is_null()
    & pl.col("ADT").is_not_null()
    & pl.col("AP02SDT").is_not_null()
    & (pl.col("ADT") >= pl.col("AP02SDT"))
)
timed1 = (
    pl.col("ADTM").is_not_null()
    & pl.col("AP01SDTM").is_not_null()
    & (pl.col("ADTM") >= pl.col("AP01SDTM"))
)
dated1 = (
    pl.col("ADTM").is_null()
    & pl.col("ADT").is_not_null()
    & pl.col("AP01SDT").is_not_null()
    & (pl.col("ADT") >= pl.col("AP01SDT"))
)

advs = (
    vs.join(adsl, on=["STUDYID", "USUBJID"], how="left", maintain_order="left")
    .with_columns(
        APERIOD=pl.when(timed2 | dated2)
        .then(2)
        .when(timed1 | dated1)
        .then(1)
        .otherwise(None)
        .cast(pl.Int64)
    )
    .with_columns(
        PARAMCD=pl.col("VSTESTCD"),
        AVAL=pl.col("VSSTRESN"),
        TRTA=pl.when(pl.col("APERIOD") == 1)
        .then(pl.col("TRT01A"))
        .when(pl.col("APERIOD") == 2)
        .then(pl.col("TRT02A"))
        .otherwise(None),
    )
    .select(
        "STUDYID", "USUBJID", "VSSEQ", "PARAMCD", "AVAL", "ADT", "ADTM",
        "APERIOD", "TRTA",
    )
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
advs.write_csv("/app/output/advs.csv")
