# Reference solution for the yamaa benchmark adam-adsl-xpt (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

adsl_raw = pl.read_csv("/app/input/adsl_raw.csv", infer_schema=False)

# A collected time without seconds names second zero.
moment = pl.col("TRTSDTC").str.to_datetime(strict=False)
adsl = adsl_raw.with_columns(
    AGE=pl.col("AGE").cast(pl.Int64),
    HEIGHTBL=pl.col("HEIGHTBL").cast(pl.Float64),
    TRTSDTM=moment.dt.strftime("%Y-%m-%dT%H:%M:%S"),
    TRTSDT=moment.dt.strftime("%Y-%m-%d"),
).select("STUDYID", "USUBJID", "TRT01A", "AGE", "HEIGHTBL", "TRTSDT", "TRTSDTM")

Path("/app/output").mkdir(exist_ok=True)
adsl.write_csv("/app/output/adsl.csv")
