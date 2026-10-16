# Reference solution for the yamaa benchmark adam-adeg-qtc-flags (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

raw = pl.read_csv("/app/input/adeg_raw.csv", infer_schema=False).with_columns(
    AVALN=pl.col("AVAL").cast(pl.Float64, strict=False),
    CHGN=pl.col("CHG").cast(pl.Float64, strict=False),
)

# Only on-treatment QTcF records are assessed, and values are compared
# unrounded, strictly above each limit: exactly at a limit reads N.
assessed = (pl.col("PARAMCD") == "QTCF").fill_null(False) & (
    pl.col("ONTRTFL") == "Y"
).fill_null(False)
absolute = assessed & pl.col("AVALN").is_not_null()
change = assessed & pl.col("CHGN").is_not_null()


def criterion(condition, text):
    return pl.when(condition).then(pl.lit(text))


def flag(condition, value, limit):
    return (
        pl.when(~condition)
        .then(None)
        .when(pl.col(value) > limit)
        .then(pl.lit("Y"))
        .otherwise(pl.lit("N"))
    )


adeg = raw.with_columns(
    CRIT1=criterion(absolute, "QTcF > 450 ms"),
    CRIT1FL=flag(absolute, "AVALN", 450),
    CRIT2=criterion(absolute, "QTcF > 480 ms"),
    CRIT2FL=flag(absolute, "AVALN", 480),
    CRIT3=criterion(absolute, "QTcF > 500 ms"),
    CRIT3FL=flag(absolute, "AVALN", 500),
    CRIT4=criterion(change, "QTcF increase from baseline > 30 ms"),
    CRIT4FL=flag(change, "CHGN", 30),
    CRIT5=criterion(change, "QTcF increase from baseline > 60 ms"),
    CRIT5FL=flag(change, "CHGN", 60),
).select(
    "STUDYID",
    "USUBJID",
    "PARAMCD",
    "PARAM",
    "AVISIT",
    "AVAL",
    "ABLFL",
    "ONTRTFL",
    "BASE",
    "CHG",
    "CRIT1",
    "CRIT1FL",
    "CRIT2",
    "CRIT2FL",
    "CRIT3",
    "CRIT3FL",
    "CRIT4",
    "CRIT4FL",
    "CRIT5",
    "CRIT5FL",
)

Path("/app/output").mkdir(exist_ok=True)
adeg.write_csv("/app/output/adeg.csv")
