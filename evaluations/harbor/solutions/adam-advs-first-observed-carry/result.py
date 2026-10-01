# Reference solution for the yamaa benchmark adam-advs-first-observed-carry (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

vs = pl.read_csv("/app/input/vs.csv", infer_schema=False).with_columns(
    pl.col("AVISITN").cast(pl.Int64, strict=False),
    pl.col("AVAL").cast(pl.Float64, strict=False),
)

# The first visit with a result for each subject and parameter.
firsts = (
    vs.filter(pl.col("AVAL").is_not_null())
    .sort("AVISITN")
    .unique(["STUDYID", "USUBJID", "PARAMCD"], keep="first", maintain_order=True)
    .select("STUDYID", "USUBJID", "PARAMCD", FIRST_AVAL="AVAL", FIRST_VISIT="AVISITN")
)

# The first result, repeated on that and every later visit; visits before
# any result stay missing.
advs = (
    vs.join(firsts, on=["STUDYID", "USUBJID", "PARAMCD"], how="left")
    .with_columns(
        BASEVAL=pl.when(pl.col("AVISITN") >= pl.col("FIRST_VISIT"))
        .then(pl.col("FIRST_AVAL"))
        .otherwise(None)
    )
    .sort(["USUBJID", "PARAMCD", "AVISITN"])
    .select("STUDYID", "USUBJID", "PARAMCD", "AVISITN", "AVAL", "BASEVAL")
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
advs.write_csv("/app/output/advs.csv")
