# Reference solution for the yamaa benchmark adam-adeg-time-matched-baseline (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

COLUMNS = [
    "STUDYID", "USUBJID", "PARAMCD", "PARAM", "APERIOD", "APERDAY", "ATPT",
    "ATPTN", "AVAL", "AVALU", "ABLFL", "BASE", "CHG", "BASETYPE",
]
MATCH = ["STUDYID", "USUBJID", "PARAMCD", "APERIOD", "ATPTN"]

adeg = pl.read_csv(
    "/app/input/adeg_raw.csv",
    infer_schema=False,
).with_columns(
    pl.col("APERIOD").cast(pl.Int64),
    pl.col("APERDAY").cast(pl.Int64),
    pl.col("ATPTN").cast(pl.Float64),
    pl.col("AVAL").cast(pl.Float64),
)

# Day -1 of each period is that period's baseline day.
on_baseline_day = pl.col("APERDAY") == -1

# Time-matched: the same subject, parameter, period and nominal time on the
# period's own baseline day; nothing stands in for a missing one.
baseline = adeg.filter(on_baseline_day).select(
    *MATCH, pl.col("AVAL").alias("BASE")
)

adeg = (
    adeg.join(baseline, on=MATCH, how="left", validate="m:1",
              maintain_order="left")
    .with_columns(
        ABLFL=pl.when(on_baseline_day).then(pl.lit("Y")),
        # No change on the baseline day itself, nor without a baseline.
        CHG=pl.when(~on_baseline_day).then(pl.col("AVAL") - pl.col("BASE")),
        BASETYPE=pl.concat_str(
            pl.lit("Period "),
            pl.col("APERIOD").cast(pl.String),
            pl.lit(" Baseline Day "),
            pl.col("ATPT"),
        ),
    )
    .select(COLUMNS)
)

Path("/app/output").mkdir(exist_ok=True)
adeg.write_csv("/app/output/adeg.csv")
