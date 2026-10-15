# Reference solution for the yamaa benchmark adam-adeg-replicate-qtc-average (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

eg = pl.read_csv("/app/input/eg.csv", infer_schema=False).with_columns(
    RESULT=pl.col("EGSTRESN").cast(pl.Float64, strict=False),
    TPTNUM=pl.col("EGTPTNUM").cast(pl.Float64, strict=False),
)
ecg_keys = ["STUDYID", "USUBJID", "TPTNUM", "EGREPNUM"]
point_keys = ["STUDYID", "USUBJID", "TPTNUM", "EGTPT"]

qt = eg.filter(pl.col("EGTESTCD") == "QTAG")
rr = eg.filter(pl.col("EGTESTCD") == "RRAG")

# One ECG is one replicate number at a time point: its QT pairs with the RR
# of the same replicate number, never by collection time.
pairs = qt.select(*point_keys, "EGREPNUM", QT="RESULT").join(
    rr.select(*ecg_keys, RR="RESULT"), on=ecg_keys, how="left"
)
# Fridericia's correction of each ECG; zero RR cannot supply a corrected value.
pairs = pairs.with_columns(
    QTCF=pl.col("QT")
    / (pl.when(pl.col("RR") != 0).then(pl.col("RR")) / 1000).pow(1 / 3)
)


def averages(frame: pl.DataFrame, value: str, paramcd: str, param: str):
    # The mean over the ECGs that have a value; no value, no record.
    return (
        frame.filter(pl.col(value).is_not_null())
        .group_by(point_keys, maintain_order=True)
        .agg(AVAL=pl.col(value).mean())
        .with_columns(PARAMCD=pl.lit(paramcd), PARAM=pl.lit(param))
    )


adeg = (
    pl.concat(
        [
            averages(qt, "RESULT", "QTAG", "QT Interval, Aggregate (msec)"),
            averages(rr, "RESULT", "RRAG", "RR Interval, Aggregate (msec)"),
            averages(pairs, "QTCF", "QTCF", "QTcF Interval, Aggregate (msec)"),
        ]
    )
    .with_columns(
        ATPT=pl.col("EGTPT"),
        ATPTN=pl.col("TPTNUM"),
        DTYPE=pl.lit("AVERAGE"),
    )
    .sort(["STUDYID", "USUBJID", "PARAMCD", "ATPTN"])
    .select("STUDYID", "USUBJID", "PARAMCD", "PARAM", "ATPT", "ATPTN", "DTYPE", "AVAL")
)

Path("/app/output").mkdir(exist_ok=True)
adeg.write_csv("/app/output/adeg.csv")
