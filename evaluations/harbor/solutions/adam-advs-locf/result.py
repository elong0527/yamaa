# Reference solution for the yamaa benchmark adam-advs-locf (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

vs = pl.read_csv("/app/input/vs.csv", infer_schema=False).with_columns(
    pl.col("AVISITN").cast(pl.Int64, strict=False),
    pl.col("AVALCOL").cast(pl.Float64, strict=False),
)

# The collected value, otherwise the closest earlier visit with one, in
# visit-number order; a gap before the first stays missing.
advs = (
    vs.sort(["USUBJID", "PARAMCD", "AVISITN"])
    .with_columns(
        AVAL=pl.col("AVALCOL")
        .fill_null(strategy="forward")
        .over(["USUBJID", "PARAMCD"])
    )
    .select("USUBJID", "PARAMCD", "AVISITN", "AVAL")
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
advs.write_csv("/app/output/advs.csv")
